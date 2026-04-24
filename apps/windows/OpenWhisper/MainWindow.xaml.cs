using Microsoft.UI.Xaml;
using Microsoft.UI.Dispatching;
using OpenWhisper.Dictation;
using OpenWhisper.Hotkey;
using OpenWhisper.TextInjection;

namespace OpenWhisper;

public sealed partial class MainWindow : Window
{
    private readonly DictationService _service;
    private readonly GlobalHotkey _hotkey;
    private readonly DispatcherQueueTimer _refreshTimer;

    public MainWindow()
    {
        InitializeComponent();
        ExtendsContentIntoTitleBar = true;
        SetTitleBar(AppTitleBar);
        AppWindow.SetIcon("Assets/AppIcon.ico");

        var dispatcher = DispatcherQueue.GetForCurrentThread();
        _service = new DictationService(dispatcher, TextInjector.Inject);
        _service.StateChanged += (_, _) => RefreshFromCore();

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
    }

    private void OnRecordClick(object sender, RoutedEventArgs e) => _service.Toggle();

    private void RefreshFromCore()
    {
        var snap = Core.Snapshot();
        StatusText.Text = Core.StatusMessage();
        TranscriptText.Text = string.IsNullOrEmpty(Core.Transcript())
            ? "Transcript will appear here."
            : Core.Transcript();
        LevelMeter.Value = snap.IsRecording != 0 ? Core.AudioCurrentLevel() : 0;
        RecordButton.IsEnabled = snap.CanToggle != 0;
        RecordButtonText.Text = snap.IsRecording != 0 ? "Stop recording" : "Start recording";
    }
}
