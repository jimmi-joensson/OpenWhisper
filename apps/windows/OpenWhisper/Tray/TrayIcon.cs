using System.Drawing;
using System.Runtime.InteropServices;
using OpenWhisper.Util;

namespace OpenWhisper.Tray;

/// <summary>
/// Windows system-tray ("notification area") icon. Windows equivalent of
/// macOS's <c>NSStatusItem</c> — the persistent dictation-state indicator
/// outside the main window. Wraps Shell_NotifyIcon with a hidden
/// message-only window for callbacks, so we don't take a dep on a third-party
/// NotifyIcon NuGet that might drift with WinUI SDK updates.
///
/// Emits <see cref="LeftDoubleClicked"/> (open main window) and
/// <see cref="RightClicked"/> (show context menu — wired in TASK-30).
/// Icon + tooltip are swapped by the owner in response to dictation phase
/// changes.
/// </summary>
internal sealed class TrayIcon : IDisposable
{
    private const uint WM_APP_TRAY = 0x8000 + 1; // WM_APP + 1, reserved for Shell_NotifyIcon callback
    private const uint TRAY_ICON_UID = 1;

    private const uint NIM_ADD = 0x00000000;
    private const uint NIM_MODIFY = 0x00000001;
    private const uint NIM_DELETE = 0x00000002;
    private const uint NIM_SETVERSION = 0x00000004;

    private const uint NIF_MESSAGE = 0x00000001;
    private const uint NIF_ICON = 0x00000002;
    private const uint NIF_TIP = 0x00000004;
    private const uint NIF_SHOWTIP = 0x00000080;

    private const uint NOTIFYICON_VERSION_4 = 4;

    // Win32 message codes surfaced via the callback. With NOTIFYICON_VERSION_4
    // they arrive in the low word of lParam; wParam holds screen coordinates.
    private const uint WM_LBUTTONDBLCLK = 0x0203;
    private const uint WM_RBUTTONUP = 0x0205;
    private const uint WM_CONTEXTMENU = 0x007B;
    private const uint NIN_SELECT = 0x0400; // single left-click in v4

    private readonly IntPtr _hwnd;
    private readonly WndProcDelegate _wndProc; // keep GC-alive — Shell dispatches via function pointer
    private Icon? _currentIcon;
    private string _currentTooltip;
    private bool _added;
    private bool _disposed;

    public event EventHandler? LeftDoubleClicked;
    public event EventHandler<Point>? RightClicked;

    public TrayIcon(string tooltip, Icon initialIcon)
    {
        _wndProc = WndProc;
        _hwnd = CreateMessageWindow();

        _currentIcon = initialIcon;
        _currentTooltip = tooltip;

        if (!AddOrUpdateIcon(isAdd: true))
        {
            throw new InvalidOperationException("Shell_NotifyIcon add failed — Explorer may not be ready.");
        }
        _added = true;
    }

    public void UpdateIcon(Icon icon)
    {
        if (_disposed) return;
        var old = _currentIcon;
        _currentIcon = icon;
        AddOrUpdateIcon(isAdd: false);
        old?.Dispose();
    }

    public void UpdateTooltip(string tooltip)
    {
        if (_disposed) return;
        _currentTooltip = tooltip;
        AddOrUpdateIcon(isAdd: false);
    }

    private bool AddOrUpdateIcon(bool isAdd)
    {
        var data = new NOTIFYICONDATAW
        {
            cbSize = (uint)Marshal.SizeOf<NOTIFYICONDATAW>(),
            hWnd = _hwnd,
            uID = TRAY_ICON_UID,
            uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP,
            uCallbackMessage = WM_APP_TRAY,
            hIcon = _currentIcon?.Handle ?? IntPtr.Zero,
            szTip = _currentTooltip ?? string.Empty,
            szInfo = string.Empty,
            szInfoTitle = string.Empty,
        };

        if (!Shell_NotifyIcon(isAdd ? NIM_ADD : NIM_MODIFY, ref data))
        {
            int err = Marshal.GetLastWin32Error();
            SpikeLog.Log($"TrayIcon: Shell_NotifyIcon {(isAdd ? "ADD" : "MODIFY")} failed err={err}");
            return false;
        }

        if (isAdd)
        {
            var versionData = new NOTIFYICONDATAW
            {
                cbSize = (uint)Marshal.SizeOf<NOTIFYICONDATAW>(),
                hWnd = _hwnd,
                uID = TRAY_ICON_UID,
                uVersion = NOTIFYICON_VERSION_4,
                szTip = string.Empty,
                szInfo = string.Empty,
                szInfoTitle = string.Empty,
            };
            if (!Shell_NotifyIcon(NIM_SETVERSION, ref versionData))
            {
                SpikeLog.Log($"TrayIcon: NIM_SETVERSION failed err={Marshal.GetLastWin32Error()}");
            }
        }
        return true;
    }

    private IntPtr WndProc(IntPtr hwnd, uint msg, IntPtr wParam, IntPtr lParam)
    {
        if (msg == WM_APP_TRAY)
        {
            uint notification = (uint)(lParam.ToInt64() & 0xFFFF);

            switch (notification)
            {
                case WM_LBUTTONDBLCLK:
                case NIN_SELECT:
                    LeftDoubleClicked?.Invoke(this, EventArgs.Empty);
                    break;

                case WM_CONTEXTMENU:
                case WM_RBUTTONUP:
                    int x = (short)(wParam.ToInt64() & 0xFFFF);
                    int y = (short)((wParam.ToInt64() >> 16) & 0xFFFF);
                    RightClicked?.Invoke(this, new Point(x, y));
                    break;
            }

            return IntPtr.Zero;
        }

        return DefWindowProc(hwnd, msg, wParam, lParam);
    }

    private IntPtr CreateMessageWindow()
    {
        string className = "OpenWhisperTrayMsg_" + Guid.NewGuid().ToString("N");
        var wc = new WNDCLASSEX
        {
            cbSize = (uint)Marshal.SizeOf<WNDCLASSEX>(),
            lpfnWndProc = Marshal.GetFunctionPointerForDelegate(_wndProc),
            hInstance = GetModuleHandle(null),
            lpszClassName = className,
        };
        ushort atom = RegisterClassEx(ref wc);
        if (atom == 0)
        {
            throw new InvalidOperationException(
                $"RegisterClassEx failed: {Marshal.GetLastWin32Error()}");
        }

        IntPtr hwnd = CreateWindowEx(
            0, className, string.Empty, 0,
            0, 0, 0, 0,
            new IntPtr(-3) /* HWND_MESSAGE */, IntPtr.Zero, wc.hInstance, IntPtr.Zero);

        if (hwnd == IntPtr.Zero)
        {
            throw new InvalidOperationException(
                $"CreateWindowEx failed: {Marshal.GetLastWin32Error()}");
        }
        return hwnd;
    }

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;

        if (_added)
        {
            var del = new NOTIFYICONDATAW
            {
                cbSize = (uint)Marshal.SizeOf<NOTIFYICONDATAW>(),
                hWnd = _hwnd,
                uID = TRAY_ICON_UID,
                szTip = string.Empty,
                szInfo = string.Empty,
                szInfoTitle = string.Empty,
            };
            Shell_NotifyIcon(NIM_DELETE, ref del);
        }

        if (_hwnd != IntPtr.Zero) DestroyWindow(_hwnd);
        _currentIcon?.Dispose();
    }

    // --- P/Invoke ---

    private delegate IntPtr WndProcDelegate(IntPtr hwnd, uint msg, IntPtr wParam, IntPtr lParam);

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct WNDCLASSEX
    {
        public uint cbSize;
        public uint style;
        public IntPtr lpfnWndProc;
        public int cbClsExtra;
        public int cbWndExtra;
        public IntPtr hInstance;
        public IntPtr hIcon;
        public IntPtr hCursor;
        public IntPtr hbrBackground;
        [MarshalAs(UnmanagedType.LPWStr)] public string? lpszMenuName;
        [MarshalAs(UnmanagedType.LPWStr)] public string lpszClassName;
        public IntPtr hIconSm;
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct NOTIFYICONDATAW
    {
        public uint cbSize;
        public IntPtr hWnd;
        public uint uID;
        public uint uFlags;
        public uint uCallbackMessage;
        public IntPtr hIcon;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)]
        public string szTip;
        public uint dwState;
        public uint dwStateMask;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 256)]
        public string szInfo;
        public uint uVersion; // union with uTimeout
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 64)]
        public string szInfoTitle;
        public uint dwInfoFlags;
        public Guid guidItem;
        public IntPtr hBalloonIcon;
    }

    [DllImport("shell32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool Shell_NotifyIcon(uint dwMessage, ref NOTIFYICONDATAW pnid);

    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern ushort RegisterClassEx(ref WNDCLASSEX lpWndClass);

    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern IntPtr CreateWindowEx(
        uint dwExStyle, string lpClassName, string lpWindowName, uint dwStyle,
        int x, int y, int nWidth, int nHeight,
        IntPtr hWndParent, IntPtr hMenu, IntPtr hInstance, IntPtr lpParam);

    [DllImport("user32.dll")]
    private static extern IntPtr DefWindowProc(IntPtr hWnd, uint Msg, IntPtr wParam, IntPtr lParam);

    [DllImport("user32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool DestroyWindow(IntPtr hWnd);

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern IntPtr GetModuleHandle(string? lpModuleName);
}
