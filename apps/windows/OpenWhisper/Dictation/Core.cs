using System.Runtime.InteropServices;
using System.Text;

namespace OpenWhisper.Dictation;

/// <summary>
/// Snapshot of the Rust dictation state machine. Shape mirrors
/// <c>core/src/ffi_c.rs::OwDictationSnapshot</c>; keep them in sync.
/// </summary>
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

internal enum DictationPhase : uint
{
    Idle = 0,
    LoadingModel = 1,
    Recording = 2,
    Transcribing = 3,
    Done = 4,
    Error = 5,
}

internal enum ToggleAction : uint
{
    Ignore = 0,
    BeginRecording = 1,
    StopRecording = 2,
}

/// <summary>
/// P/Invoke surface onto <c>openwhisper_core.dll</c>. Mirrors the FFI surface
/// in <c>core/src/ffi_c.rs</c>. All strings cross the boundary as UTF-8 via
/// explicit byte buffers — default LPStr marshaling mangles non-ASCII.
///
/// The core holds singleton global state (dictation phase, audio engine).
/// All methods here are thread-safe on the Rust side; callers don't need
/// to serialize.
/// </summary>
internal static class Core
{
    private const string Dll = "openwhisper_core";

    // --- P/Invoke declarations ---

    [DllImport(Dll, EntryPoint = "ow_core_version")]
    private static extern IntPtr ow_core_version();

    [DllImport(Dll, EntryPoint = "ow_process_transcript")]
    private static extern nint ow_process_transcript(byte[] input, byte[]? outBuf, nuint outCap);

    [DllImport(Dll, EntryPoint = "ow_dictation_snapshot")]
    private static extern void ow_dictation_snapshot(out OwDictationSnapshot snap);

    [DllImport(Dll, EntryPoint = "ow_dictation_status_message")]
    private static extern nint ow_dictation_status_message(byte[]? outBuf, nuint outCap);

    [DllImport(Dll, EntryPoint = "ow_dictation_transcript")]
    private static extern nint ow_dictation_transcript(byte[]? outBuf, nuint outCap);

    [DllImport(Dll, EntryPoint = "ow_dictation_error_message")]
    private static extern nint ow_dictation_error_message(byte[]? outBuf, nuint outCap);

    [DllImport(Dll, EntryPoint = "ow_dictation_request_toggle")]
    private static extern uint ow_dictation_request_toggle();

    [DllImport(Dll, EntryPoint = "ow_dictation_request_cancel")]
    private static extern byte ow_dictation_request_cancel();

    [DllImport(Dll, EntryPoint = "ow_dictation_mark_loading_model")]
    private static extern void ow_dictation_mark_loading_model();

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

    [DllImport(Dll, EntryPoint = "ow_audio_is_capturing")]
    private static extern byte ow_audio_is_capturing();

    [DllImport(Dll, EntryPoint = "ow_audio_current_level")]
    private static extern float ow_audio_current_level();

    [DllImport(Dll, EntryPoint = "ow_audio_drain_samples")]
    private static extern nint ow_audio_drain_samples(float[]? outBuf, nuint outCap);


    // --- Public wrappers ---

    public static string Version => Marshal.PtrToStringUTF8(ow_core_version()) ?? string.Empty;

    public static string ProcessTranscript(string input)
    {
        var inBytes = EncodeCString(input);
        return CallIntoByteBuffer((buf, cap) => ow_process_transcript(inBytes, buf, cap));
    }

    public static OwDictationSnapshot Snapshot()
    {
        ow_dictation_snapshot(out var s);
        return s;
    }

    public static string StatusMessage() =>
        CallIntoByteBuffer((buf, cap) => ow_dictation_status_message(buf, cap));

    public static string Transcript() =>
        CallIntoByteBuffer((buf, cap) => ow_dictation_transcript(buf, cap));

    public static string ErrorMessage() =>
        CallIntoByteBuffer((buf, cap) => ow_dictation_error_message(buf, cap));

    public static ToggleAction RequestToggle() => (ToggleAction)ow_dictation_request_toggle();
    public static bool RequestCancel() => ow_dictation_request_cancel() != 0;

    public static void MarkLoadingModel() => ow_dictation_mark_loading_model();
    public static void MarkCaptureStarted() => ow_dictation_mark_capture_started();
    public static void MarkCaptureStopped(ulong sampleCount) => ow_dictation_mark_capture_stopped(sampleCount);

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
    public static bool AudioIsCapturing() => ow_audio_is_capturing() != 0;
    public static float AudioCurrentLevel() => ow_audio_current_level();

    /// <summary>
    /// One-shot drain with a generous 16 MB float buffer (~4 minutes of 16 kHz
    /// audio). The FFI drains its internal Vec inside the call, so probing
    /// with cap=0 would be destructive — single big call is the safe pattern.
    /// </summary>
    public static float[] AudioDrainSamples()
    {
        var buf = new float[4_000_000];
        nint written = ow_audio_drain_samples(buf, (nuint)buf.Length);
        if (written < 0)
        {
            // Callers treat this as silent — better than a crash. Shouldn't
            // happen in practice unless someone records >4 minutes.
            return Array.Empty<float>();
        }
        if (written == 0) return Array.Empty<float>();
        return buf[..(int)written];
    }


    // --- FFI string helpers ---

    private static byte[] EncodeCString(string s)
    {
        int len = Encoding.UTF8.GetByteCount(s);
        var buf = new byte[len + 1];
        Encoding.UTF8.GetBytes(s, 0, s.Length, buf, 0);
        buf[len] = 0;
        return buf;
    }

    /// <summary>
    /// Rust contract for byte-buffer writes: returns bytes-written (≥0) or
    /// <c>-(required_capacity)</c> on overflow. Probe with cap=0 to learn
    /// size, then call again with the sized buffer.
    /// </summary>
    private static string CallIntoByteBuffer(Func<byte[]?, nuint, nint> call)
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
