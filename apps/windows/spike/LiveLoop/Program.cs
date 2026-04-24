// Spike: end-to-end dictation loop on Windows without any UI.
//
// Wires together the two earlier spikes into the orchestration pattern the
// real WinUI 3 shell will use:
//   - Rust core (openwhisper_core.dll) owns audio capture + phase machine.
//   - C# owns ONNX inference via sherpa-onnx.
//   - State transitions are driven by core's `ow_dictation_*` entry points;
//     the shell is a thin observer that pulls samples, calls the recognizer,
//     and pushes the resulting transcript back.
//
// Flow:
//   1. Press Enter to start recording (hotkey comes later — Win32 hotkey spike).
//   2. While recording, poll `ow_audio_current_level` for a level meter.
//   3. Press Enter again to stop; drain samples; transcribe via sherpa-onnx.
//   4. Deliver transcript back into core; print the final snapshot.
//
// Success = the printed transcript round-trips through core's post-processing
// (fillers stripped, "open whisper" → "OpenWhisper") before being displayed.

using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;
using OpenWhisper.Spike.LiveLoop;
using SherpaOnnx;

const string ModelName = "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8";

// --seconds N: record for N seconds, transcribe once, exit. Non-interactive
// smoke path for CI / scripted tests. No N → the interactive Enter-to-toggle
// flow below.
double? fixedDuration = null;
for (int i = 0; i < args.Length; i++)
{
    if (args[i] == "--seconds" && i + 1 < args.Length && double.TryParse(args[i + 1], System.Globalization.CultureInfo.InvariantCulture, out var s))
    {
        fixedDuration = s;
    }
}

string modelDir = Path.Combine(
    Environment.GetFolderPath(Environment.SpecialFolder.UserProfile),
    ".cache", "openwhisper", "models", ModelName);

if (!Directory.Exists(modelDir))
{
    Console.Error.WriteLine($"Model not found at {modelDir}.");
    Console.Error.WriteLine("Run the OnnxSmokeTest spike first — it downloads the weights on first launch.");
    return 1;
}

Console.WriteLine($"[live] core version : {Core.GetVersion()}");
Console.WriteLine($"[live] model dir    : {modelDir}");

// Initialize the ASR recognizer once. Model load is the expensive step
// (~2.6 s on this box); keeping it in memory makes repeat iterations snappy.
using var recognizer = BuildRecognizer(modelDir);
Console.WriteLine("[live] recognizer ready");
Console.WriteLine();

if (fixedDuration is double seconds)
{
    RunOneIteration(recognizer, timedSeconds: seconds);
    return 0;
}

while (true)
{
    Console.Write("[live] press Enter to START recording (or 'q' + Enter to quit): ");
    var key = Console.ReadLine();
    if (string.Equals(key, "q", StringComparison.OrdinalIgnoreCase)) break;

    RunOneIteration(recognizer, timedSeconds: null);
    Console.WriteLine();
}

return 0;


static OfflineRecognizer BuildRecognizer(string modelDir)
{
    // sherpa-onnx native side reads paths as UTF-8 but the C# bindings marshal
    // as ANSI. Without the 8.3 short-path trick, `ø` in the user profile path
    // corrupts the filenames and CreateOfflineRecognizer SEH-crashes.
    string encoder = PathTricks.ToShortPath(Path.Combine(modelDir, "encoder.int8.onnx"));
    string decoder = PathTricks.ToShortPath(Path.Combine(modelDir, "decoder.int8.onnx"));
    string joiner  = PathTricks.ToShortPath(Path.Combine(modelDir, "joiner.int8.onnx"));
    string tokens  = PathTricks.ToShortPath(Path.Combine(modelDir, "tokens.txt"));

    var config = new OfflineRecognizerConfig();
    config.ModelConfig.Transducer.Encoder = encoder;
    config.ModelConfig.Transducer.Decoder = decoder;
    config.ModelConfig.Transducer.Joiner = joiner;
    config.ModelConfig.Tokens = tokens;
    config.ModelConfig.ModelType = "nemo_transducer";
    config.ModelConfig.NumThreads = 1;
    config.ModelConfig.Provider = "cpu";
    config.DecodingMethod = "greedy_search";

    return new OfflineRecognizer(config);
}


static void RunOneIteration(OfflineRecognizer recognizer, double? timedSeconds)
{
    uint action = Core.RequestToggle();
    if (action != 1) // TOGGLE_BEGIN_RECORDING
    {
        Console.WriteLine($"[live] unexpected toggle action: {action} — aborting iteration");
        return;
    }

    // Start the mic. Core owns the cpal worker (WASAPI backend on Windows).
    if (!Core.AudioStartCapture(out string err))
    {
        Console.WriteLine($"[live] mic start failed: {err}");
        Core.DeliverError(err);
        return;
    }
    Core.MarkCaptureStarted();
    var recordStart = Stopwatch.GetTimestamp();

    // Level meter loop: polls the lock-free level atomic from core every 50 ms
    // and renders a simple bar. Exits when the user hits Enter again (or
    // when the fixed timer expires in --seconds mode).
    using var cts = new CancellationTokenSource();
    var levelTask = Task.Run(() => RenderLevelMeter(recordStart, cts.Token));

    if (timedSeconds is double dur)
    {
        Console.WriteLine($"[live] recording for {dur:F1} s (non-interactive)…");
        Thread.Sleep(TimeSpan.FromSeconds(dur));
    }
    else
    {
        Console.Write("[live] recording — press Enter to STOP: ");
        Console.ReadLine();
    }
    cts.Cancel();
    try { levelTask.Wait(500); } catch { /* background task shutdown */ }
    Console.WriteLine(); // clear the meter line

    // Stop + drain. Core holds any samples captured after Stop until the
    // next Drain call — so ordering here is tolerant of jitter.
    Core.AudioStopCapture();
    float[] samples = Core.AudioDrainSamples();
    Core.MarkCaptureStopped((ulong)samples.Length);

    if (samples.Length == 0)
    {
        Console.WriteLine("[live] no audio captured — nothing to transcribe");
        return;
    }

    double seconds = samples.Length / 16000.0;
    Console.WriteLine($"[live] captured  : {samples.Length} samples ({seconds:F2} s @ 16 kHz)");

    // Inference. Same pattern as OnnxSmokeTest.
    var inferStart = Stopwatch.GetTimestamp();
    using var stream = recognizer.CreateStream();
    stream.AcceptWaveform(16000, samples);
    recognizer.Decode(stream);
    var inferMs = Stopwatch.GetElapsedTime(inferStart).TotalMilliseconds;
    var rtf = (seconds * 1000) / inferMs;
    string rawText = stream.Result.Text;

    Console.WriteLine($"[live] decoded   : {inferMs:F0} ms ({rtf:F1}x realtime)");
    Console.WriteLine($"[live] raw text  : \"{rawText}\"");

    // Push the transcript back into core. This triggers the post-processing
    // pipeline (filler stripping, substitutions) and transitions to DONE.
    // We keep a reasonable confidence placeholder — sherpa's OfflineResult
    // doesn't expose a confidence, and we'd derive one from TDT decode stats
    // in a real build.
    Core.DeliverTranscript(rawText, confidence: 0.90f);

    var snap = Core.GetSnapshot();
    Console.WriteLine($"[live] post-proc : \"{Core.GetTranscript()}\"");
    Console.WriteLine($"[live] status    : {Core.GetStatusMessage()} (phase={snap.Phase}, conf={snap.Confidence:F2})");
}


static void RenderLevelMeter(long startTicks, CancellationToken ct)
{
    var bar = new StringBuilder();
    while (!ct.IsCancellationRequested)
    {
        float level = Core.AudioCurrentLevel();
        // 0..1 → 0..20 blocks; clamp for safety against transient > 1.0 values.
        int blocks = Math.Clamp((int)MathF.Round(level * 20f), 0, 20);
        var elapsed = Stopwatch.GetElapsedTime(startTicks);
        bar.Clear();
        bar.Append('\r').Append("[live] level: [");
        bar.Append('█', blocks);
        bar.Append(' ', 20 - blocks);
        bar.Append($"] {level:F2}  t={elapsed.TotalSeconds:F1}s   ");
        Console.Write(bar.ToString());
        try { Task.Delay(50, ct).Wait(ct); } catch (OperationCanceledException) { break; }
    }
}


namespace OpenWhisper.Spike.LiveLoop
{
    [StructLayout(LayoutKind.Sequential)]
    internal struct OwDictationSnapshot
    {
        public uint Phase;
        public float Confidence;
        public ulong SampleCount;
        public ulong ElapsedMs;
        public byte CanToggle;
        public byte IsRecording;
    }

    internal static class Core
    {
        private const string Dll = "openwhisper_core";

        [DllImport(Dll, EntryPoint = "ow_core_version")]
        private static extern IntPtr ow_core_version();

        [DllImport(Dll, EntryPoint = "ow_dictation_snapshot")]
        private static extern void ow_dictation_snapshot(out OwDictationSnapshot snap);

        [DllImport(Dll, EntryPoint = "ow_dictation_status_message")]
        private static extern nint ow_dictation_status_message(byte[]? outBuf, nuint outCap);

        [DllImport(Dll, EntryPoint = "ow_dictation_transcript")]
        private static extern nint ow_dictation_transcript(byte[]? outBuf, nuint outCap);

        [DllImport(Dll, EntryPoint = "ow_dictation_request_toggle")]
        private static extern uint ow_dictation_request_toggle();

        [DllImport(Dll, EntryPoint = "ow_dictation_mark_capture_started")]
        private static extern void ow_dictation_mark_capture_started();

        [DllImport(Dll, EntryPoint = "ow_dictation_mark_capture_stopped")]
        private static extern void ow_dictation_mark_capture_stopped(ulong sampleCount);

        [DllImport(Dll, EntryPoint = "ow_dictation_deliver_transcript")]
        private static extern void ow_dictation_deliver_transcript(byte[] text, float confidence);

        [DllImport(Dll, EntryPoint = "ow_dictation_deliver_error")]
        private static extern void ow_dictation_deliver_error(byte[] message);

        [DllImport(Dll, EntryPoint = "ow_audio_start_capture")]
        private static extern int ow_audio_start_capture(byte[]? errBuf, nuint errCap);

        [DllImport(Dll, EntryPoint = "ow_audio_stop_capture")]
        private static extern void ow_audio_stop_capture();

        [DllImport(Dll, EntryPoint = "ow_audio_current_level")]
        private static extern float ow_audio_current_level();

        [DllImport(Dll, EntryPoint = "ow_audio_drain_samples")]
        private static extern nint ow_audio_drain_samples(float[]? outBuf, nuint outCap);

        public static string GetVersion() => Marshal.PtrToStringUTF8(ow_core_version()) ?? string.Empty;

        public static OwDictationSnapshot GetSnapshot()
        {
            ow_dictation_snapshot(out var s);
            return s;
        }

        public static string GetStatusMessage() =>
            CallIntoBuffer((buf, cap) => ow_dictation_status_message(buf, cap));

        public static string GetTranscript() =>
            CallIntoBuffer((buf, cap) => ow_dictation_transcript(buf, cap));

        public static uint RequestToggle() => ow_dictation_request_toggle();
        public static void MarkCaptureStarted() => ow_dictation_mark_capture_started();
        public static void MarkCaptureStopped(ulong n) => ow_dictation_mark_capture_stopped(n);

        public static void DeliverTranscript(string text, float confidence) =>
            ow_dictation_deliver_transcript(EncodeCString(text), confidence);

        public static void DeliverError(string message) =>
            ow_dictation_deliver_error(EncodeCString(message));

        public static bool AudioStartCapture(out string errorMessage)
        {
            var errBuf = new byte[512];
            int rc = ow_audio_start_capture(errBuf, (nuint)errBuf.Length);
            if (rc == 0)
            {
                errorMessage = string.Empty;
                return true;
            }
            int len = Array.IndexOf<byte>(errBuf, 0);
            if (len < 0) len = errBuf.Length;
            errorMessage = Encoding.UTF8.GetString(errBuf, 0, len);
            return false;
        }

        public static void AudioStopCapture() => ow_audio_stop_capture();
        public static float AudioCurrentLevel() => ow_audio_current_level();

        public static float[] AudioDrainSamples()
        {
            // One-shot with a generous buffer. The Rust FFI drains its internal
            // Vec inside the call — so probing with cap=0 and retrying is
            // destructive, not just a size query. 16 MB = ~4 min at 16 kHz,
            // well above any plausible single utterance for this spike.
            var buf = new float[4_000_000];
            nint written = ow_audio_drain_samples(buf, (nuint)buf.Length);
            if (written < 0)
            {
                int needed = checked((int)-written);
                Console.Error.WriteLine($"[live] WARN: {needed} samples exceeded drain buffer ({buf.Length}); audio lost");
                return Array.Empty<float>();
            }
            return written == 0 ? Array.Empty<float>() : buf[..(int)written];
        }


        private static byte[] EncodeCString(string s)
        {
            int len = Encoding.UTF8.GetByteCount(s);
            var buf = new byte[len + 1];
            Encoding.UTF8.GetBytes(s, 0, s.Length, buf, 0);
            buf[len] = 0;
            return buf;
        }

        private static string CallIntoBuffer(Func<byte[]?, nuint, nint> call)
        {
            nint probe = call(null, 0);
            int required = probe < 0 ? checked((int)-probe) : checked((int)probe) + 1;
            if (required <= 1) return string.Empty;
            var buf = new byte[required];
            nint written = call(buf, (nuint)buf.Length);
            if (written < 0) return string.Empty;
            return Encoding.UTF8.GetString(buf, 0, (int)written);
        }
    }

    internal static class PathTricks
    {
        public static string ToShortPath(string longPath)
        {
            if (!OperatingSystem.IsWindows()) return longPath;
            var buf = new StringBuilder(260);
            int len = GetShortPathNameW(longPath, buf, buf.Capacity);
            return (len == 0 || len > buf.Capacity) ? longPath : buf.ToString();
        }

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern int GetShortPathNameW(string lpszLongPath, StringBuilder lpszShortPath, int cchBuffer);
    }
}
