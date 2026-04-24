using System.Runtime.InteropServices;
using OpenWhisper.Util;

namespace OpenWhisper.Hotkey;

/// <summary>
/// Low-level keyboard hook dedicated to Escape-to-cancel. Runs on its own
/// thread with a private message pump so main-thread stalls can't starve the
/// callback — Windows silently unloads hooks whose callbacks exceed
/// <c>LowLevelHooksTimeout</c> (default ~300 ms). Mirrors the Escape side
/// of macOS's CGEventTap: when the user hits Escape anywhere, we fire
/// <see cref="EscapePressed"/>. The subscriber (DictationService) phase-gates
/// the action via the Rust core — Cancel is a no-op unless we're recording.
///
/// <para>
/// Scope is deliberately narrow. Windows hotkey activation stays <c>Ctrl+Space</c>
/// via <c>RegisterHotKey</c> (<see cref="GlobalHotkey"/>). Don't generalize this
/// into a full keyboard-hook service — that would tempt porting Mac's
/// tap-not-hold semantics to Windows, which is a platform-convention choice
/// we're deliberately not making (see <c>docs/claude-windows-handoff.md</c>
/// and the feedback memory on per-platform hotkey choice).
/// </para>
/// </summary>
internal sealed class EscapeHook : IDisposable
{
    private const int WH_KEYBOARD_LL = 13;
    private const int VK_ESCAPE = 0x1B;
    private const uint WM_KEYDOWN = 0x0100;
    private const uint WM_SYSKEYDOWN = 0x0104;
    private const uint WM_QUIT = 0x0012;

    private readonly LowLevelKeyboardProc _proc; // keep the delegate GC-alive — Shell dispatches via function pointer
    private readonly ManualResetEventSlim _ready = new(initialState: false);
    private Thread? _thread;
    private IntPtr _hookHandle;
    private uint _threadId;
    private bool _disposed;

    public event EventHandler? EscapePressed;

    public EscapeHook()
    {
        _proc = HookCallback;
    }

    /// <summary>
    /// Install the hook on a dedicated thread. Returns true if the hook
    /// installed successfully; false if <c>SetWindowsHookEx</c> failed
    /// (e.g. blocked by AV).
    /// </summary>
    public bool Start()
    {
        if (_thread is not null) return _hookHandle != IntPtr.Zero;

        _thread = new Thread(RunHookThread)
        {
            IsBackground = true,
            Name = "OpenWhisper.EscapeHook",
        };
        _thread.Start();
        _ready.Wait();
        return _hookHandle != IntPtr.Zero;
    }

    private void RunHookThread()
    {
        _threadId = GetCurrentThreadId();
        _hookHandle = SetWindowsHookExW(
            WH_KEYBOARD_LL, _proc, GetModuleHandle(null), dwThreadId: 0);

        if (_hookHandle == IntPtr.Zero)
        {
            SpikeLog.Log($"EscapeHook: SetWindowsHookEx failed err={Marshal.GetLastWin32Error()}");
            _ready.Set();
            return;
        }

        _ready.Set();

        // Private message pump: required for WH_KEYBOARD_LL callback dispatch
        // on this thread. Exits when Dispose posts WM_QUIT.
        while (GetMessage(out MSG msg, IntPtr.Zero, 0, 0) > 0)
        {
            TranslateMessage(ref msg);
            DispatchMessage(ref msg);
        }

        UnhookWindowsHookEx(_hookHandle);
        _hookHandle = IntPtr.Zero;
    }

    private IntPtr HookCallback(int nCode, IntPtr wParam, IntPtr lParam)
    {
        // Keep this callback tiny. Windows unloads hooks whose callbacks
        // exceed LowLevelHooksTimeout (default ~300 ms). Everything expensive
        // hops to the UI thread via the subscriber.
        if (nCode >= 0)
        {
            uint msg = (uint)wParam;
            if (msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN)
            {
                var info = Marshal.PtrToStructure<KBDLLHOOKSTRUCT>(lParam);
                if (info.vkCode == VK_ESCAPE)
                {
                    EscapePressed?.Invoke(this, EventArgs.Empty);
                }
            }
        }
        // NEVER swallow Escape — every other app in Windows expects it to
        // reach its focused handler. Always chain to the next hook.
        return CallNextHookEx(IntPtr.Zero, nCode, wParam, lParam);
    }

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;

        if (_threadId != 0)
        {
            PostThreadMessage(_threadId, WM_QUIT, IntPtr.Zero, IntPtr.Zero);
        }
        _thread?.Join(millisecondsTimeout: 500);
        _ready.Dispose();
    }

    // --- P/Invoke ---

    private delegate IntPtr LowLevelKeyboardProc(int nCode, IntPtr wParam, IntPtr lParam);

    [StructLayout(LayoutKind.Sequential)]
    private struct KBDLLHOOKSTRUCT
    {
        public uint vkCode;
        public uint scanCode;
        public uint flags;
        public uint time;
        public IntPtr dwExtraInfo;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct MSG
    {
        public IntPtr hwnd;
        public uint message;
        public IntPtr wParam;
        public IntPtr lParam;
        public uint time;
        public int pt_x;
        public int pt_y;
    }

    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern IntPtr SetWindowsHookExW(int idHook, LowLevelKeyboardProc lpfn, IntPtr hMod, uint dwThreadId);

    [DllImport("user32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool UnhookWindowsHookEx(IntPtr hhk);

    [DllImport("user32.dll")]
    private static extern IntPtr CallNextHookEx(IntPtr hhk, int nCode, IntPtr wParam, IntPtr lParam);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern int GetMessage(out MSG lpMsg, IntPtr hWnd, uint wMsgFilterMin, uint wMsgFilterMax);

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool TranslateMessage(ref MSG lpMsg);

    [DllImport("user32.dll")]
    private static extern IntPtr DispatchMessage(ref MSG lpMsg);

    [DllImport("user32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool PostThreadMessage(uint idThread, uint Msg, IntPtr wParam, IntPtr lParam);

    [DllImport("kernel32.dll")]
    private static extern uint GetCurrentThreadId();

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern IntPtr GetModuleHandle(string? lpModuleName);
}
