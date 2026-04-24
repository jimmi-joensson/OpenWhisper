using Microsoft.UI.Xaml;
using Microsoft.UI.Dispatching;
using OpenWhisper.Dictation;
using OpenWhisper.Hotkey;
using OpenWhisper.TextInjection;
using OpenWhisper.Util;

namespace OpenWhisper;

public sealed partial class MainWindow : Window
{
    private readonly DictationService _service;
    private readonly GlobalHotkey _hotkey;
    private readonly DispatcherQueueTimer _refreshTimer;

    public MainWindow()
    {
        SpikeLog.Log("MainWindow ctor entered");
        InitializeComponent();
        ExtendsContentIntoTitleBar = true;
        SetTitleBar(AppTitleBar);
        AppWindow.SetIcon("Assets/AppIcon.ico");
        SpikeLog.Log($"MainWindow ctor: core version from ffi = {Core.Version}");

        var dispatcher = DispatcherQueue.GetForCurrentThread();
        _service = new DictationService(dispatcher, (text, hwnd) => TextInjector.Inject(text, hwnd));
        _service.StateChanged += (_, _) => RefreshFromCore();
        _service.ModelLoadProgressChanged += (_, _) => RefreshModelLoad();

        _hotkey = new GlobalHotkey(this);
        _hotkey.Pressed += (_, _) => _service.Toggle();
        if (!_hotkey.Register())
        {
            // Another app owns Ctrl+Space. Surface it inline rather than crashing.
            StatusText.Text = "couldn't register Left Ctrl + Space — another app may own it";
        }

        // 20 Hz UI refresh for status + level meter. Same cadence the handoff
        // doc prescribes ("shells poll (20 Hz with Mutex<State> is trivially
        // cheap)"). The state machine itself is push-driven; this just drives
        // the level bar and the elapsed-time display.
        _refreshTimer = dispatcher.CreateTimer();
        _refreshTimer.Interval = TimeSpan.FromMilliseconds(50);
        _refreshTimer.Tick += (_, _) => RefreshFromCore();
        _refreshTimer.Start();

        Closed += (_, _) =>
        {
            _refreshTimer.Stop();
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
