using System.Runtime.InteropServices;
using OpenWhisper.Util;

namespace OpenWhisper.TextInjection;

/// <summary>
/// Paste transcribed text into whichever window currently has keyboard focus.
///
/// Uses a clipboard-based paste (set clipboard → SendInput Ctrl+V → restore
/// clipboard) rather than per-character Unicode SendInput. Tried SendInput
/// first; on this machine only the first few characters reached the target
/// before focus wobbled and the rest got absorbed by OpenWhisper's own
/// window. Clipboard paste is a single atomic operation the receiving app
/// handles in one shot, so focus races mid-stream can't eat half the text.
///
/// We save and restore the clipboard around the paste so the user's
/// existing clipboard contents aren't destroyed. Lossy for non-text formats
/// (images, files) — worth revisiting later, but the common case is text.
///
/// Mirrors <c>apps/macos/App/TextInjector.swift</c>'s CGEvent-based paste
/// on the Mac side in concept: OS-native synthesized paste.
/// </summary>
internal static class TextInjector
{
    /// <summary>Snapshot the currently-focused window so we can restore it before pasting.</summary>
    public static IntPtr CaptureForegroundWindow()
    {
        var hwnd = GetForegroundWindow();
        SpikeLog.Log($"TextInjector: captured foreground HWND=0x{hwnd.ToInt64():X}");
        return hwnd;
    }

    public static void Inject(string text, IntPtr targetHwnd)
    {
        if (string.IsNullOrEmpty(text)) return;

        string? savedClipboard = null;
        try { savedClipboard = TryGetClipboardText(); }
        catch (Exception ex) { SpikeLog.Log($"Inject: save clipboard failed: {ex.Message}"); }

        if (!TrySetClipboardText(text))
        {
            SpikeLog.Log("Inject: FAILED to set clipboard — aborting paste");
            return;
        }

        if (targetHwnd != IntPtr.Zero)
        {
            bool restored = SetForegroundWindow(targetHwnd);
            SpikeLog.Log($"Inject: SetForegroundWindow(0x{targetHwnd.ToInt64():X}) → {restored}");
            // Brief settle so the target has time to become the keyboard-focus
            // owner before we send Ctrl+V.
            Thread.Sleep(40);
        }

        SendCtrlV();
        // Give the target a moment to process the paste before we clobber
        // the clipboard back to its original contents. If we restore too
        // fast, Ctrl+V may resolve against the restored clipboard.
        Thread.Sleep(100);

        if (savedClipboard is not null)
        {
            try { TrySetClipboardText(savedClipboard); }
            catch (Exception ex) { SpikeLog.Log($"Inject: restore clipboard failed: {ex.Message}"); }
        }
        SpikeLog.Log("Inject: paste sequence complete");
    }


    // --- Clipboard helpers (Win32) ---

    private static string? TryGetClipboardText()
    {
        if (!OpenClipboard(IntPtr.Zero)) return null;
        try
        {
            IntPtr h = GetClipboardData(CF_UNICODETEXT);
            if (h == IntPtr.Zero) return null;
            IntPtr p = GlobalLock(h);
            if (p == IntPtr.Zero) return null;
            try { return Marshal.PtrToStringUni(p); }
            finally { GlobalUnlock(h); }
        }
        finally { CloseClipboard(); }
    }

    private static bool TrySetClipboardText(string text)
    {
        int bytes = (text.Length + 1) * 2; // UTF-16 + null terminator
        IntPtr hGlobal = GlobalAlloc(GMEM_MOVEABLE, (UIntPtr)bytes);
        if (hGlobal == IntPtr.Zero) return false;
        IntPtr lockedPtr = GlobalLock(hGlobal);
        if (lockedPtr == IntPtr.Zero)
        {
            GlobalFree(hGlobal);
            return false;
        }
        try
        {
            Marshal.Copy(System.Text.Encoding.Unicode.GetBytes(text + '\0'), 0, lockedPtr, bytes);
        }
        finally { GlobalUnlock(hGlobal); }

        if (!OpenClipboard(IntPtr.Zero))
        {
            GlobalFree(hGlobal);
            return false;
        }
        try
        {
            EmptyClipboard();
            if (SetClipboardData(CF_UNICODETEXT, hGlobal) == IntPtr.Zero)
            {
                GlobalFree(hGlobal);
                return false;
            }
            // Ownership of hGlobal transferred to the clipboard on success.
            return true;
        }
        finally { CloseClipboard(); }
    }


    // --- SendInput Ctrl+V ---

    private static void SendCtrlV()
    {
        const ushort VK_CONTROL = 0x11;
        const ushort VK_V = 0x56;

        var inputs = new INPUT[4];
        // Ctrl down
        inputs[0].type = INPUT_KEYBOARD;
        inputs[0].U.ki = new KEYBDINPUT { wVk = VK_CONTROL };
        // V down
        inputs[1].type = INPUT_KEYBOARD;
        inputs[1].U.ki = new KEYBDINPUT { wVk = VK_V };
        // V up
        inputs[2].type = INPUT_KEYBOARD;
        inputs[2].U.ki = new KEYBDINPUT { wVk = VK_V, dwFlags = KEYEVENTF_KEYUP };
        // Ctrl up
        inputs[3].type = INPUT_KEYBOARD;
        inputs[3].U.ki = new KEYBDINPUT { wVk = VK_CONTROL, dwFlags = KEYEVENTF_KEYUP };

        uint sent = SendInput((uint)inputs.Length, inputs, Marshal.SizeOf<INPUT>());
        SpikeLog.Log($"Inject: SendInput Ctrl+V sent {sent}/4");
    }


    // --- P/Invoke ---

    private const uint INPUT_KEYBOARD = 1;
    private const uint KEYEVENTF_KEYUP = 0x0002;

    private const uint CF_UNICODETEXT = 13;
    private const uint GMEM_MOVEABLE = 0x0002;

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
        [FieldOffset(0)] public MOUSEINPUT mi;
        [FieldOffset(0)] public HARDWAREINPUT hi;
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

    [StructLayout(LayoutKind.Sequential)]
    private struct MOUSEINPUT
    {
        public int dx;
        public int dy;
        public uint mouseData;
        public uint dwFlags;
        public uint time;
        public IntPtr dwExtraInfo;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct HARDWAREINPUT
    {
        public uint uMsg;
        public ushort wParamL;
        public ushort wParamH;
    }

    [DllImport("user32.dll", SetLastError = true)]
    private static extern uint SendInput(uint nInputs, [MarshalAs(UnmanagedType.LPArray)] INPUT[] pInputs, int cbSize);

    [DllImport("user32.dll")]
    private static extern IntPtr GetForegroundWindow();

    [DllImport("user32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool SetForegroundWindow(IntPtr hWnd);

    [DllImport("user32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool OpenClipboard(IntPtr hWndNewOwner);

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool CloseClipboard();

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool EmptyClipboard();

    [DllImport("user32.dll")]
    private static extern IntPtr GetClipboardData(uint uFormat);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern IntPtr SetClipboardData(uint uFormat, IntPtr hMem);

    [DllImport("kernel32.dll")]
    private static extern IntPtr GlobalAlloc(uint uFlags, UIntPtr dwBytes);

    [DllImport("kernel32.dll")]
    private static extern IntPtr GlobalFree(IntPtr hMem);

    [DllImport("kernel32.dll")]
    private static extern IntPtr GlobalLock(IntPtr hMem);

    [DllImport("kernel32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GlobalUnlock(IntPtr hMem);
}
