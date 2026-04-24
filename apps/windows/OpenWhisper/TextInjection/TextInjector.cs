using System.Runtime.InteropServices;

namespace OpenWhisper.TextInjection;

/// <summary>
/// Paste transcribed text into whichever window currently has keyboard focus.
///
/// Uses <c>SendInput</c> with KEYEVENTF_UNICODE so any Unicode character goes
/// through without caring about the target app's current keyboard layout.
/// That handles Danish / emoji / anything Parakeet can emit. The clipboard
/// alternative (set clipboard, send Ctrl+V) would clobber the user's
/// clipboard; SendInput avoids that at the cost of being slightly slower
/// for very long strings.
///
/// Mirrors <c>apps/macos/App/TextInjector.swift</c> — that one uses CGEvent
/// to post synthesized keystrokes. Same idea, different OS.
/// </summary>
internal static class TextInjector
{
    private const uint INPUT_KEYBOARD = 1;
    private const uint KEYEVENTF_KEYUP = 0x0002;
    private const uint KEYEVENTF_UNICODE = 0x0004;

    [StructLayout(LayoutKind.Sequential)]
    private struct INPUT
    {
        public uint type;
        public InputUnion U;
    }

    [StructLayout(LayoutKind.Explicit)]
    private struct InputUnion
    {
        [FieldOffset(0)] public KEYBDINPUT ki;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct KEYBDINPUT
    {
        public ushort wVk;
        public ushort wScan;
        public uint dwFlags;
        public uint time;
        public IntPtr dwExtraInfo;
    }

    public static void Inject(string text)
    {
        if (string.IsNullOrEmpty(text)) return;

        // Each UTF-16 code unit becomes a down+up pair. Surrogate pairs are
        // fine — Windows treats them as two separate KEYEVENTF_UNICODE events
        // and the receiving app reassembles. We send them in a single
        // SendInput batch so consumers see the string atomically where they
        // can, though apps are free to receive characters one at a time.
        var inputs = new INPUT[text.Length * 2];
        int i = 0;
        foreach (var ch in text)
        {
            inputs[i].type = INPUT_KEYBOARD;
            inputs[i].U.ki = new KEYBDINPUT { wScan = ch, dwFlags = KEYEVENTF_UNICODE };
            inputs[i + 1].type = INPUT_KEYBOARD;
            inputs[i + 1].U.ki = new KEYBDINPUT { wScan = ch, dwFlags = KEYEVENTF_UNICODE | KEYEVENTF_KEYUP };
            i += 2;
        }
        _ = SendInput((uint)inputs.Length, inputs, Marshal.SizeOf<INPUT>());
    }

    [DllImport("user32.dll", SetLastError = true)]
    private static extern uint SendInput(uint nInputs, [MarshalAs(UnmanagedType.LPArray)] INPUT[] pInputs, int cbSize);
}
