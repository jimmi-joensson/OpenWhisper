// Spike: prove the Rust core's C ABI (core/src/ffi_c.rs) is reachable from
// C# via P/Invoke on this machine. Mirror of the swift-bridge path the
// macOS shell uses; Windows shell will eventually consume the same functions.
//
// Success criteria:
// - openwhisper_core.dll loads
// - ow_core_version returns the cargo package version
// - ow_process_transcript strips fillers round-trip through FFI
// - Snapshot + toggle flow mutates and reports shared state coherently

using System.Runtime.InteropServices;
using System.Text;
using OpenWhisper.Spike.CoreInterop;

Console.WriteLine($"[interop] core version   : {Core.GetVersion()}");

string raw = "um, this is, uh, a round-trip test of open whisper";
Console.WriteLine($"[interop] transcript in  : \"{raw}\"");
Console.WriteLine($"[interop] transcript out : \"{Core.ProcessTranscript(raw)}\"");

Console.WriteLine();
Console.WriteLine("[interop] dictation flow:");
PrintSnapshot("initial         ");

uint toggle1 = Core.RequestToggle();
Console.WriteLine($"[interop] toggle #1 -> {FormatToggle(toggle1)}");
Core.MarkCaptureStarted();
PrintSnapshot("after start     ");

uint toggle2 = Core.RequestToggle();
Console.WriteLine($"[interop] toggle #2 -> {FormatToggle(toggle2)}");
Core.MarkCaptureStopped(80_000);
PrintSnapshot("after stop 80k  ");

Core.DeliverTranscript("Hello from P/Invoke", 0.87f);
PrintSnapshot("after deliver   ");

return 0;

static void PrintSnapshot(string label)
{
    var snap = Core.GetSnapshot();
    string phase = snap.Phase switch
    {
        0 => "idle",
        1 => "loading",
        2 => "recording",
        3 => "transcribing",
        4 => "done",
        5 => "error",
        _ => $"?{snap.Phase}",
    };
    Console.WriteLine(
        $"  {label}  phase={phase}  canToggle={snap.CanToggle != 0}  recording={snap.IsRecording != 0}  " +
        $"conf={snap.Confidence:F2}  samples={snap.SampleCount}  status=\"{Core.GetStatusMessage()}\"  " +
        $"transcript=\"{Core.GetTranscript()}\"");
}

static string FormatToggle(uint action) => action switch
{
    0 => "IGNORE",
    1 => "BEGIN",
    2 => "STOP",
    _ => $"UNKNOWN({action})",
};


namespace OpenWhisper.Spike.CoreInterop
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

    /// <summary>
    /// Thin wrapper over the C ABI in core/src/ffi_c.rs.
    ///
    /// Strings cross the boundary as UTF-8 via explicit byte buffers — we avoid
    /// LPStr marshaling because the default Windows ANSI codepage mangles
    /// non-ASCII (same root cause as the JimmiJønsson path bug in the other
    /// spike). Rust returns bytes-written-or-required-capacity; we size the
    /// buffer in a grow loop.
    /// </summary>
    internal static class Core
    {
        private const string Dll = "openwhisper_core";

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

        [DllImport(Dll, EntryPoint = "ow_dictation_request_toggle")]
        private static extern uint ow_dictation_request_toggle();

        [DllImport(Dll, EntryPoint = "ow_dictation_mark_capture_started")]
        private static extern void ow_dictation_mark_capture_started();

        [DllImport(Dll, EntryPoint = "ow_dictation_mark_capture_stopped")]
        private static extern void ow_dictation_mark_capture_stopped(ulong sampleCount);

        [DllImport(Dll, EntryPoint = "ow_dictation_deliver_transcript")]
        private static extern void ow_dictation_deliver_transcript(byte[] text, float confidence);

        public static string GetVersion() => PtrToStringUtf8(ow_core_version());

        public static string ProcessTranscript(string input)
        {
            var inBytes = EncodeCString(input);
            return CallIntoBuffer((buf, cap) => ow_process_transcript(inBytes, buf, cap));
        }

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

        public static void MarkCaptureStopped(ulong sampleCount) =>
            ow_dictation_mark_capture_stopped(sampleCount);

        public static void DeliverTranscript(string text, float confidence) =>
            ow_dictation_deliver_transcript(EncodeCString(text), confidence);


        // --- FFI helpers ---

        private static byte[] EncodeCString(string s)
        {
            int len = Encoding.UTF8.GetByteCount(s);
            var buf = new byte[len + 1];
            Encoding.UTF8.GetBytes(s, 0, s.Length, buf, 0);
            buf[len] = 0;
            return buf;
        }

        /// <summary>
        /// Calls a C ABI function that writes UTF-8 into a caller buffer.
        /// Rust contract: returns bytes-written (≥0) or -(required_capacity)
        /// on overflow. We probe with cap=0 to learn the size, then call
        /// again with a sized buffer.
        /// </summary>
        private static string CallIntoBuffer(Func<byte[]?, nuint, nint> call)
        {
            // Probe. Required capacity includes the null terminator.
            nint probe = call(null, 0);
            int required = probe < 0 ? checked((int)-probe) : checked((int)probe) + 1;
            if (required <= 1) return string.Empty;

            var buf = new byte[required];
            nint written = call(buf, (nuint)buf.Length);
            if (written < 0)
            {
                // Someone raced us and the required size grew mid-call.
                // Rare; a single retry would cover it. For the spike, just
                // return empty and let the caller log.
                return string.Empty;
            }
            return Encoding.UTF8.GetString(buf, 0, (int)written);
        }

        private static string PtrToStringUtf8(IntPtr ptr)
        {
            if (ptr == IntPtr.Zero) return string.Empty;
            return Marshal.PtrToStringUTF8(ptr) ?? string.Empty;
        }
    }
}
