using System.Runtime.InteropServices;
using Microsoft.UI;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Input;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Shapes;
using OpenWhisper.Dictation;
using OpenWhisper.Util;
using Windows.Graphics;

namespace OpenWhisper;

/// <summary>
/// Floating, borderless, always-on-top HUD pill — Windows equivalent of
/// <c>apps/macos/App/PillOverlay.swift</c>. Three states visible to the user:
/// idle dots → recording level meter → transcribing spinner. Click-through
/// while active so the user can keep dictating into whatever app has focus;
/// clickable while idle to bring the main window forward.
///
/// Geometry + motion values come from shared tokens in <c>App.xaml</c>
/// (see <c>docs/design/identity-tokens.md</c>). Don't hardcode values here
/// that duplicate the tokens — pull them from resources instead.
/// </summary>
public sealed partial class PillWindow : Window
{
    private enum PillStatus { Idle, Recording, Transcribing }

    private readonly DictationService _service;
    private readonly Action _showMainWindow;
    private readonly DispatcherQueue _dispatcher;
    private readonly DispatcherQueueTimer _tickTimer;

    private readonly int _barCount;
    private readonly double _barMaxHeight;
    private readonly double _barMinHeight;
    private readonly double _barSpacing;
    private readonly double _floorDb;
    private readonly int _pollIntervalMs;
    private readonly int _graceReturnMs;
    private readonly double _pillWidthDip;
    private readonly double _pillHeightDip;
    private readonly double _pillGapDip;

    private readonly Rectangle[] _bars;
    private readonly float[] _levelHistory;

    private PillStatus _status = PillStatus.Idle;
    private DictationPhase _lastPhase = DictationPhase.Idle;
    private CancellationTokenSource? _graceCts;
    private bool _fullscreenHidden;

    /// <summary>
    /// Raised on the UI thread when the foreground-app-is-fullscreen state
    /// flips. Subscribers piggyback on the pill's 20 Hz poll for their own
    /// fullscreen-sensitive behavior (e.g. MainWindow disables the global
    /// hotkey while fullscreen is active).
    /// </summary>
    public event EventHandler<bool>? FullscreenChanged;

    internal PillWindow(DictationService service, Action showMainWindow)
    {
        _service = service;
        _showMainWindow = showMainWindow;
        _dispatcher = DispatcherQueue.GetForCurrentThread();

        InitializeComponent();

        var resources = Application.Current.Resources;
        _barCount = (int)resources["PillLevelMeterBarCount"];
        _barMaxHeight = (double)resources["PillLevelMeterHeight"];
        _barMinHeight = (double)resources["LevelMeterBarMinHeight"];
        _barSpacing = (double)resources["LevelMeterBarSpacing"];
        _floorDb = (double)resources["LevelMeterFloorDb"];
        _pollIntervalMs = (int)resources["LevelMeterPollIntervalMs"];
        _graceReturnMs = (int)resources["GraceReturnToIdleMs"];
        _pillWidthDip = (double)resources["PillWidth"];
        _pillHeightDip = (double)resources["PillHeight"];
        _pillGapDip = (double)resources["PillGapAboveTaskbar"];

        _bars = new Rectangle[_barCount];
        _levelHistory = new float[_barCount];

        ConfigureWindow();
        BuildBars();
        ApplyStatus(PillStatus.Idle, immediate: true);
        PositionAboveTaskbar();

        _tickTimer = _dispatcher.CreateTimer();
        _tickTimer.Interval = TimeSpan.FromMilliseconds(_pollIntervalMs);
        _tickTimer.Tick += (_, _) => Tick();
        _tickTimer.Start();

        _service.StateChanged += OnServiceStateChanged;

        Closed += (_, _) =>
        {
            _tickTimer.Stop();
            _service.StateChanged -= OnServiceStateChanged;
            _graceCts?.Cancel();
        };
    }

    // --- Window shape / placement ---

    private void ConfigureWindow()
    {
        var hwnd = WinRT.Interop.WindowNative.GetWindowHandle(this);
        var appWindowId = Win32Interop.GetWindowIdFromWindow(hwnd);
        var appWindow = AppWindow.GetFromWindowId(appWindowId);

        if (appWindow.Presenter is OverlappedPresenter presenter)
        {
            presenter.SetBorderAndTitleBar(false, false);
            presenter.IsMaximizable = false;
            presenter.IsMinimizable = false;
            presenter.IsResizable = false;
            presenter.IsAlwaysOnTop = true;
        }

        // WS_EX_TOOLWINDOW hides from Alt-Tab + taskbar.
        // WS_EX_NOACTIVATE stops the pill from stealing focus when it shows up.
        SetExStyleBits(hwnd, PillInterop.WS_EX_TOOLWINDOW | PillInterop.WS_EX_NOACTIVATE, enable: true);

        // Win11 will otherwise auto-round our window corners by ~8 px, which
        // conflicts with the larger capsule radius we apply ourselves below.
        // Telling DWM not to round lets the window region define the shape.
        int noRound = PillInterop.DWMWCP_DONOTROUND;
        PillInterop.DwmSetWindowAttribute(hwnd, PillInterop.DWMWA_WINDOW_CORNER_PREFERENCE, ref noRound, sizeof(int));

        ResizeForDpi(hwnd, appWindow);
        ApplyCapsuleRegion(hwnd, appWindow);
    }

    /// <summary>
    /// Clip the window to a capsule (stadium) shape via <c>SetWindowRgn</c>.
    /// Without this, the window is rectangular and the <c>Border</c>'s
    /// <c>CornerRadius</c> only rounds the rendered content — the window
    /// chrome (or acrylic backdrop's rectangular footprint) still shows as
    /// a halo behind the capsule. Region-clipping makes the window itself
    /// capsule-shaped so there's no rectangle to leak.
    /// </summary>
    private void ApplyCapsuleRegion(IntPtr hwnd, AppWindow appWindow)
    {
        var size = appWindow.Size;
        // CreateRoundRectRgn takes the ellipse axis lengths (diameters), not
        // radii. For a full capsule, both axes equal the window height.
        IntPtr rgn = PillInterop.CreateRoundRectRgn(0, 0, size.Width + 1, size.Height + 1, size.Height, size.Height);
        if (rgn == IntPtr.Zero) return;
        // After SetWindowRgn succeeds, the system owns the region — don't delete.
        PillInterop.SetWindowRgn(hwnd, rgn, bRedraw: true);
    }

    private void ResizeForDpi(IntPtr hwnd, AppWindow appWindow)
    {
        uint dpi = PillInterop.GetDpiForWindow(hwnd);
        double scale = dpi / 96.0;
        var size = new SizeInt32(
            (int)Math.Round(_pillWidthDip * scale),
            (int)Math.Round(_pillHeightDip * scale));
        appWindow.Resize(size);
    }

    private void PositionAboveTaskbar()
    {
        var hwnd = WinRT.Interop.WindowNative.GetWindowHandle(this);
        var appWindowId = Win32Interop.GetWindowIdFromWindow(hwnd);
        var appWindow = AppWindow.GetFromWindowId(appWindowId);

        // WorkArea excludes the taskbar on Windows, same way macOS's
        // `visibleFrame` excludes the Dock. So the bottom of the work area
        // is always just above whatever part of the shell is reserved.
        var display = DisplayArea.GetFromWindowId(appWindowId, DisplayAreaFallback.Nearest);
        var work = display.WorkArea;

        uint dpi = PillInterop.GetDpiForWindow(hwnd);
        double scale = dpi / 96.0;
        int gapPx = (int)Math.Round(_pillGapDip * scale);

        int x = work.X + (work.Width - appWindow.Size.Width) / 2;
        int y = work.Y + work.Height - appWindow.Size.Height - gapPx;
        appWindow.Move(new PointInt32(x, y));
    }

    private void SetClickThrough(bool clickThrough)
    {
        var hwnd = WinRT.Interop.WindowNative.GetWindowHandle(this);
        SetExStyleBits(
            hwnd,
            PillInterop.WS_EX_TRANSPARENT | PillInterop.WS_EX_LAYERED,
            enable: clickThrough);
    }

    private static void SetExStyleBits(IntPtr hwnd, long bits, bool enable)
    {
        long ex = PillInterop.GetWindowLongPtr(hwnd, PillInterop.GWL_EXSTYLE).ToInt64();
        ex = enable ? (ex | bits) : (ex & ~bits);
        PillInterop.SetWindowLongPtr(hwnd, PillInterop.GWL_EXSTYLE, new IntPtr(ex));
    }

    // --- Level meter plumbing ---

    private void BuildBars()
    {
        var padding = (Thickness)Application.Current.Resources["PillPadding"];
        double innerWidth = _pillWidthDip - padding.Left - padding.Right;
        double totalSpacing = _barSpacing * (_barCount - 1);
        double barWidth = Math.Max(1, (innerWidth - totalSpacing) / _barCount);
        var radius = (CornerRadius)Application.Current.Resources["LevelMeterBarCornerRadius"];
        var inactive = (SolidColorBrush)Application.Current.Resources["LevelMeterInactiveBrush"];

        for (int i = 0; i < _barCount; i++)
        {
            var bar = new Rectangle
            {
                Width = barWidth,
                Height = _barMinHeight,
                RadiusX = radius.TopLeft,
                RadiusY = radius.TopLeft,
                VerticalAlignment = VerticalAlignment.Center,
                Fill = inactive,
            };
            _bars[i] = bar;
            LevelBars.Children.Add(bar);
        }
    }

    private void Tick()
    {
        // Mac gets fullscreen hiding for free because fullscreen apps live on
        // their own Space (see `PillOverlay.swift` collectionBehavior comment).
        // Windows has no Spaces — IsAlwaysOnTop would render the pill over
        // fullscreen games, videos, and presentations. Detect the condition
        // ourselves and hide; re-show on exit.
        bool wantHidden = IsForegroundAppFullscreen();
        if (wantHidden != _fullscreenHidden)
        {
            _fullscreenHidden = wantHidden;
            var appWindow = GetAppWindow();
            if (wantHidden) appWindow.Hide();
            else appWindow.Show();
            FullscreenChanged?.Invoke(this, wantHidden);
        }

        if (_status == PillStatus.Idle) return;

        float sample = _status == PillStatus.Recording ? Core.AudioCurrentLevel() : 0f;
        Array.Copy(_levelHistory, 1, _levelHistory, 0, _levelHistory.Length - 1);
        _levelHistory[^1] = sample;
        RefreshBars();
    }

    private AppWindow GetAppWindow()
    {
        var hwnd = WinRT.Interop.WindowNative.GetWindowHandle(this);
        return AppWindow.GetFromWindowId(Win32Interop.GetWindowIdFromWindow(hwnd));
    }

    /// <summary>
    /// True if the foreground window covers its monitor exactly — classical
    /// fullscreen games (D3D exclusive), borderless fullscreen apps, and
    /// presentation modes all match. The pill itself is skipped so we don't
    /// flicker if it's ever foreground; shell surfaces (desktop, taskbar)
    /// don't match because their window rects don't cover the full monitor.
    /// </summary>
    private bool IsForegroundAppFullscreen()
    {
        IntPtr fg = PillInterop.GetForegroundWindow();
        if (fg == IntPtr.Zero) return false;

        var myHwnd = WinRT.Interop.WindowNative.GetWindowHandle(this);
        if (fg == myHwnd) return false;

        if (!PillInterop.GetWindowRect(fg, out var winRect)) return false;

        IntPtr monitor = PillInterop.MonitorFromWindow(fg, PillInterop.MONITOR_DEFAULTTONEAREST);
        if (monitor == IntPtr.Zero) return false;

        var mi = new PillInterop.MONITORINFO { cbSize = (uint)Marshal.SizeOf<PillInterop.MONITORINFO>() };
        if (!PillInterop.GetMonitorInfo(monitor, ref mi)) return false;

        // Exact-match against the full monitor rect (including the area under
        // the taskbar). We deliberately don't compare against WorkArea — that
        // would also match ordinary maximized windows, which should NOT hide
        // the pill.
        return winRect.left <= mi.rcMonitor.left
            && winRect.top <= mi.rcMonitor.top
            && winRect.right >= mi.rcMonitor.right
            && winRect.bottom >= mi.rcMonitor.bottom;
    }

    private void RefreshBars()
    {
        var brush = _status == PillStatus.Recording
            ? (SolidColorBrush)Application.Current.Resources["OpenWhisperRecordingBrush"]
            : (SolidColorBrush)Application.Current.Resources["LevelMeterInactiveBrush"];

        for (int i = 0; i < _barCount; i++)
        {
            double norm = DbNormalize(_levelHistory[i]);
            _bars[i].Height = Math.Max(_barMinHeight, norm * _barMaxHeight);
            _bars[i].Fill = brush;
        }
    }

    private double DbNormalize(float amplitude)
    {
        double db = 20.0 * Math.Log10(Math.Max(amplitude, 1e-6));
        return Math.Clamp((db - _floorDb) / -_floorDb, 0.0, 1.0);
    }

    // --- Phase → status mapping ---

    private void OnServiceStateChanged(object? sender, EventArgs e)
    {
        var snap = Core.Snapshot();
        var phase = (DictationPhase)snap.Phase;

        PillStatus next = phase switch
        {
            DictationPhase.Recording => PillStatus.Recording,
            DictationPhase.Transcribing => PillStatus.Transcribing,
            _ => PillStatus.Idle,
        };

        if (next == _status && phase == _lastPhase) return;

        // Grace delay when we're leaving Transcribing — mirrors Mac's
        // `returnToIdleAfter(250 ms)` so the user sees the spinner finish
        // before the pill snaps back.
        bool needsGrace = _status == PillStatus.Transcribing && next == PillStatus.Idle;
        _lastPhase = phase;

        if (needsGrace)
        {
            ScheduleReturnToIdle();
        }
        else
        {
            ApplyStatus(next, immediate: true);
        }
    }

    private void ScheduleReturnToIdle()
    {
        _graceCts?.Cancel();
        _graceCts = new CancellationTokenSource();
        var token = _graceCts.Token;
        int delayMs = _graceReturnMs;
        _ = Task.Run(async () =>
        {
            try { await Task.Delay(delayMs, token); }
            catch { return; }
            if (token.IsCancellationRequested) return;
            _dispatcher.TryEnqueue(() =>
            {
                if (token.IsCancellationRequested) return;
                ApplyStatus(PillStatus.Idle, immediate: true);
            });
        });
    }

    private void ApplyStatus(PillStatus status, bool immediate)
    {
        if (immediate)
        {
            _graceCts?.Cancel();
            _graceCts = null;
        }

        _status = status;

        IdleDots.Visibility = status == PillStatus.Idle ? Visibility.Visible : Visibility.Collapsed;
        ActiveRow.Visibility = status == PillStatus.Idle ? Visibility.Collapsed : Visibility.Visible;

        bool isTranscribing = status == PillStatus.Transcribing;
        TranscribingRing.Visibility = isTranscribing ? Visibility.Visible : Visibility.Collapsed;
        TranscribingRing.IsActive = isTranscribing;

        SetClickThrough(clickThrough: status != PillStatus.Idle);

        if (status == PillStatus.Idle)
        {
            Array.Clear(_levelHistory);
            RefreshBars();
        }
    }

    // --- Pointer handling (idle state only; recording/transcribing are click-through) ---

    private void OnPillPressed(object sender, PointerRoutedEventArgs e)
    {
        if (_status != PillStatus.Idle) return;
        try
        {
            _showMainWindow();
        }
        catch (Exception ex)
        {
            SpikeLog.Log($"PillWindow: showMainWindow threw {ex}");
        }
    }

}

// Win32 interop concentrated in a nested type so it doesn't pollute the
// OpenWhisper namespace at large. Only the pill needs these right now.
internal static class PillInterop
{
    public const int GWL_EXSTYLE = -20;
    public const long WS_EX_TOOLWINDOW = 0x00000080;
    public const long WS_EX_TRANSPARENT = 0x00000020;
    public const long WS_EX_LAYERED = 0x00080000;
    public const long WS_EX_NOACTIVATE = 0x08000000;

    public const uint MONITOR_DEFAULTTONEAREST = 0x00000002;

    // DWMWA_WINDOW_CORNER_PREFERENCE (Win11 22000+). Values per DWMWINDOWATTRIBUTE.
    public const int DWMWA_WINDOW_CORNER_PREFERENCE = 33;
    public const int DWMWCP_DONOTROUND = 1;

    [StructLayout(LayoutKind.Sequential)]
    public struct RECT
    {
        public int left;
        public int top;
        public int right;
        public int bottom;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct MONITORINFO
    {
        public uint cbSize;
        public RECT rcMonitor;
        public RECT rcWork;
        public uint dwFlags;
    }

    [DllImport("user32.dll", EntryPoint = "GetWindowLongPtrW", SetLastError = true)]
    public static extern IntPtr GetWindowLongPtr(IntPtr hWnd, int nIndex);

    [DllImport("user32.dll", EntryPoint = "SetWindowLongPtrW", SetLastError = true)]
    public static extern IntPtr SetWindowLongPtr(IntPtr hWnd, int nIndex, IntPtr dwNewLong);

    [DllImport("user32.dll")]
    public static extern uint GetDpiForWindow(IntPtr hwnd);

    [DllImport("user32.dll")]
    public static extern IntPtr GetForegroundWindow();

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);

    [DllImport("user32.dll")]
    public static extern IntPtr MonitorFromWindow(IntPtr hwnd, uint dwFlags);

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static extern bool GetMonitorInfo(IntPtr hMonitor, ref MONITORINFO lpmi);

    [DllImport("gdi32.dll")]
    public static extern IntPtr CreateRoundRectRgn(int x1, int y1, int x2, int y2, int w, int h);

    [DllImport("user32.dll")]
    public static extern int SetWindowRgn(IntPtr hWnd, IntPtr hRgn, [MarshalAs(UnmanagedType.Bool)] bool bRedraw);

    [DllImport("dwmapi.dll")]
    public static extern int DwmSetWindowAttribute(IntPtr hwnd, int dwAttribute, ref int pvAttribute, int cbAttribute);
}
