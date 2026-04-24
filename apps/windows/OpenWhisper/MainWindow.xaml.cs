using System.Runtime.InteropServices;
using Microsoft.UI;
using Microsoft.UI.Xaml;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Windowing;
using OpenWhisper.Dictation;
using OpenWhisper.Hotkey;
using OpenWhisper.Settings;
using OpenWhisper.TextInjection;
using OpenWhisper.Tray;
using OpenWhisper.Util;

namespace OpenWhisper;

public sealed partial class MainWindow : Window
{
    private const int TrayIconSize = 32;
    private const int GWL_EXSTYLE = -20;
    private const long WS_EX_TOOLWINDOW = 0x00000080;

    private readonly AppSettings _settings;
    private readonly DictationService _service;
    private readonly GlobalHotkey _hotkey;
    private readonly EscapeHook _escapeHook;
    private readonly DispatcherQueueTimer _refreshTimer;
    private readonly PillWindow _pill;
    private readonly TrayIcon? _tray;
    private DictationPhase _trayPhase = DictationPhase.Idle;
    private bool _reallyClosing;

    public MainWindow()
    {
        SpikeLog.Log("MainWindow ctor entered");
        _settings = AppSettings.Load();
        InitializeComponent();
        ExtendsContentIntoTitleBar = true;
        SetTitleBar(AppTitleBar);
        AppWindow.SetIcon("Assets/AppIcon.ico");
        ApplyTaskbarVisibility();
        SpikeLog.Log($"MainWindow ctor: core version from ffi = {Core.Version}");

        var dispatcher = DispatcherQueue.GetForCurrentThread();
        _service = new DictationService(dispatcher, (text, hwnd) => TextInjector.Inject(text, hwnd));
        _service.StateChanged += (_, _) => RefreshFromCore();
        _service.ModelLoadProgressChanged += (_, _) => RefreshModelLoad();

        _hotkey = new GlobalHotkey(this);
        _hotkey.Pressed += (_, _) => _service.Toggle();
        if (!_hotkey.Register())
        {
            ShowHotkeyBlockedBanner();
        }

        // Global Escape-to-cancel: Windows analog of macOS's event-tap Escape
        // handling. Core gates the action by phase, so Escape is a no-op
        // outside of Recording. If the low-level hook fails to install
        // (AV intervention, locked-down policy), we silently lose the
        // global-Escape feature but the app keeps working — in-window
        // Escape still reaches the focused XAML tree.
        _escapeHook = new EscapeHook();
        _escapeHook.EscapePressed += (_, _) => dispatcher.TryEnqueue(_service.Cancel);
        if (!_escapeHook.Start())
        {
            SpikeLog.Log("MainWindow: EscapeHook failed to install — Escape-to-cancel disabled");
        }

        // 20 Hz UI refresh for status + level meter. Same cadence the handoff
        // doc prescribes ("shells poll (20 Hz with Mutex<State> is trivially
        // cheap)"). The state machine itself is push-driven; this just drives
        // the level bar and the elapsed-time display.
        _refreshTimer = dispatcher.CreateTimer();
        _refreshTimer.Interval = TimeSpan.FromMilliseconds(50);
        _refreshTimer.Tick += (_, _) => RefreshFromCore();
        _refreshTimer.Start();

        _pill = new PillWindow(_service, Activate);
        _pill.FullscreenChanged += OnFullscreenChanged;
        _pill.Activate();

        try
        {
            _tray = new TrayIcon("OpenWhisper — idle", StatusIconRenderer.RenderIdle(TrayIconSize));
            _tray.LeftDoubleClicked += (_, _) => dispatcher.TryEnqueue(RestoreFromTray);
            _tray.RightClicked += (_, pt) => dispatcher.TryEnqueue(() => ShowTrayMenu(pt.X, pt.Y));
            _service.StateChanged += (_, _) => RefreshTray();
        }
        catch (Exception ex)
        {
            SpikeLog.Log($"MainWindow: tray init failed: {ex}");
        }

        // Main window close → hide to tray. App keeps running; hotkey,
        // dictation, pill, and tray icon stay alive. Mac gets the same
        // behavior via `applicationShouldTerminateAfterLastWindowClosed = false`
        // plus the `.accessory` activation policy — here we intercept the
        // close on the AppWindow.
        AppWindow.Closing += OnAppWindowClosing;

        Closed += (_, _) =>
        {
            _refreshTimer.Stop();
            _tray?.Dispose();
            _pill.Close();
            _escapeHook.Dispose();
            _hotkey.Dispose();
            _service.Dispose();
        };

        RefreshFromCore();
        RefreshModelLoad();
    }

    private void OnRecordClick(object sender, RoutedEventArgs e)
    {
        SpikeLog.Log("OnRecordClick");
        _service.Toggle();
    }

    // --- Fullscreen-aware hotkey gating ---

    /// <summary>
    /// When a fullscreen app comes to the foreground, release the global
    /// Ctrl+Space hotkey so it reaches the fullscreen app normally (games
    /// that bind space, presentations, videos). Re-register when the user
    /// leaves fullscreen. The pill's <see cref="PillWindow.FullscreenChanged"/>
    /// event piggybacks on its own 20 Hz poll so this costs nothing extra.
    /// </summary>
    private void OnFullscreenChanged(object? sender, bool isFullscreen)
    {
        if (isFullscreen)
        {
            _hotkey.Unregister();
        }
        else
        {
            // Re-register. If another app grabbed Ctrl+Space during the
            // fullscreen window, this silently fails — the hotkey stays off
            // until something releases the combo. Acceptable edge case.
            _hotkey.Register();
        }
    }

    // --- Tray-only / accessory mode ---

    private void OnAppWindowClosing(AppWindow sender, AppWindowClosingEventArgs args)
    {
        if (_reallyClosing) return;
        args.Cancel = true;
        AppWindow.Hide();
    }

    /// <summary>
    /// Called when the user double-clicks the tray icon. Unhides the window
    /// (if hidden), raises it above other windows, and gives it focus.
    /// </summary>
    private void RestoreFromTray()
    {
        AppWindow.Show();
        Activate();
    }

    /// <summary>
    /// Used by TASK-30's Quit menu item (and eventual future shutdown paths)
    /// to bypass the hide-on-close behavior and terminate the process.
    /// </summary>
    internal void RequestQuit()
    {
        _reallyClosing = true;
        Close();
        Microsoft.UI.Xaml.Application.Current.Exit();
    }

    /// <summary>
    /// Apply the "Show in taskbar" setting by toggling WS_EX_TOOLWINDOW on
    /// the main HWND. Tool-window style drops the window out of both the
    /// taskbar and Alt-Tab — exactly what macOS's `.accessory` policy gives
    /// us for free. Must run after <c>InitializeComponent</c> so AppWindow /
    /// HWND exist.
    /// </summary>
    private void ApplyTaskbarVisibility()
    {
        var hwnd = WinRT.Interop.WindowNative.GetWindowHandle(this);
        long ex = GetWindowLongPtr(hwnd, GWL_EXSTYLE).ToInt64();
        ex = _settings.ShowInTaskbar
            ? ex & ~WS_EX_TOOLWINDOW
            : ex | WS_EX_TOOLWINDOW;
        SetWindowLongPtr(hwnd, GWL_EXSTYLE, new IntPtr(ex));
    }

    [DllImport("user32.dll", EntryPoint = "GetWindowLongPtrW", SetLastError = true)]
    private static extern IntPtr GetWindowLongPtr(IntPtr hWnd, int nIndex);

    [DllImport("user32.dll", EntryPoint = "SetWindowLongPtrW", SetLastError = true)]
    private static extern IntPtr SetWindowLongPtr(IntPtr hWnd, int nIndex, IntPtr dwNewLong);

    // --- Health banner (Windows analog of Mac restart banner) ---

    private void ShowHotkeyBlockedBanner()
    {
        HealthBannerText.Text = "Ctrl+Space is in use by another app. Dictation can still run from the Record button or the tray, but the global hotkey won't fire.";
        HealthBannerAction.Content = "Retry";
        HealthBanner.Visibility = Microsoft.UI.Xaml.Visibility.Visible;
    }

    private void OnHealthBannerAction(object sender, RoutedEventArgs e)
    {
        // Re-attempt hotkey registration. If another app released the combo
        // since startup, the banner hides and the hotkey starts working; if
        // the block is still in place, the banner stays visible.
        if (_hotkey.Register())
        {
            HealthBanner.Visibility = Microsoft.UI.Xaml.Visibility.Collapsed;
        }
    }

    private void RefreshFromCore()
    {
        var snap = Core.Snapshot();
        StatusText.Text = Core.StatusMessage();
        TranscriptText.Text = Core.Transcript();
        LevelMeter.Value = snap.IsRecording != 0 ? Core.AudioCurrentLevel() : 0;

        // Button enabled only when the model is loaded AND the core state
        // permits toggling. During model load, we show a disabled record
        // button with "Loading model…" so the UI is honest about why
        // clicking doesn't do anything yet.
        RecordButton.IsEnabled = _service.IsReady && snap.CanToggle != 0;
        if (!_service.IsReady)
        {
            RecordButtonText.Text = _service.LoadError is not null ? "Model load failed" : "Loading model…";
        }
        else
        {
            RecordButtonText.Text = snap.IsRecording != 0 ? "Stop recording" : "Start recording";
        }
    }

    private void RefreshTray()
    {
        if (_tray is null) return;

        var phase = (DictationPhase)Core.Snapshot().Phase;
        if (phase == _trayPhase) return;

        bool nowRecording = phase == DictationPhase.Recording;
        bool wasRecording = _trayPhase == DictationPhase.Recording;
        _trayPhase = phase;

        if (nowRecording != wasRecording)
        {
            _tray.UpdateIcon(nowRecording
                ? StatusIconRenderer.RenderRecording(TrayIconSize)
                : StatusIconRenderer.RenderIdle(TrayIconSize));
        }
        _tray.UpdateTooltip($"OpenWhisper — {TooltipFor(phase)}");
    }

    private static string TooltipFor(DictationPhase phase) => phase switch
    {
        DictationPhase.LoadingModel => "loading model",
        DictationPhase.Recording => "recording",
        DictationPhase.Transcribing => "transcribing",
        DictationPhase.Error => "error",
        _ => "idle",
    };

    // --- Tray menu (mirrors macOS menubar menu item text) ---

    private void ShowTrayMenu(int screenX, int screenY)
    {
        var phase = (DictationPhase)Core.Snapshot().Phase;
        var hwnd = WinRT.Interop.WindowNative.GetWindowHandle(this);

        var items = new TrayMenu.Item[]
        {
            new() { Text = "Open OpenWhisper", Handler = RestoreFromTray },
            TrayMenu.Item.Separator(),
            new()
            {
                Text = DictationItemTitle(phase),
                Enabled = IsInteractable(phase),
                Handler = () => _service.Toggle(),
            },
            TrayMenu.Item.Separator(),
            new() { Text = "Quit OpenWhisper", Handler = RequestQuit },
        };

        TrayMenu.Show(screenX, screenY, hwnd, items);
    }

    private static string DictationItemTitle(DictationPhase phase) => phase switch
    {
        DictationPhase.Recording => "Stop Dictation",
        DictationPhase.LoadingModel => "Loading model…",
        DictationPhase.Transcribing => "Transcribing…",
        _ => "Start Dictation",
    };

    private static bool IsInteractable(DictationPhase phase) =>
        phase != DictationPhase.LoadingModel && phase != DictationPhase.Transcribing;

    private void RefreshModelLoad()
    {
        if (_service.IsReady)
        {
            ModelLoadBar.IsOpen = false;
            RefreshFromCore(); // flip the Record button out of "Loading model…" state
            return;
        }
        if (_service.LoadError is not null)
        {
            ModelLoadBar.Severity = Microsoft.UI.Xaml.Controls.InfoBarSeverity.Error;
            ModelLoadBar.Title = "Model load failed";
            ModelLoadBar.Message = _service.LoadError.Message;
            ModelLoadBar.IsOpen = true;
            ModelLoadProgress.IsIndeterminate = false;
            ModelLoadProgress.Value = 0;
            return;
        }

        ModelLoadBar.IsOpen = true;
        var progress = _service.LastProgress;
        if (progress is { } p && p.PercentComplete is double pct)
        {
            ModelLoadBar.Title = $"Downloading Parakeet v3 multilingual — {pct:F0}%";
            ModelLoadBar.Message = $"{p.BytesReceived / 1_048_576.0:F1} / {p.TotalBytes / 1_048_576.0:F1} MB (first-run only)";
            ModelLoadProgress.IsIndeterminate = false;
            ModelLoadProgress.Value = pct;
        }
        else
        {
            // No progress report yet — either warming up the download or
            // loading cached weights. Leave the bar indeterminate.
            ModelLoadBar.Title = "Preparing Parakeet v3 (multilingual)…";
            ModelLoadBar.Message = "Loading model weights.";
            ModelLoadProgress.IsIndeterminate = true;
        }
    }
}
