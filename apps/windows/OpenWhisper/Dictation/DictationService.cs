using System.Diagnostics;
using Microsoft.UI.Dispatching;
using OpenWhisper.TextInjection;
using OpenWhisper.Util;

namespace OpenWhisper.Dictation;

/// <summary>
/// Orchestrates the full dictation flow on Windows.
///
/// Mirrors <c>apps/macos/App/DictationService.swift</c> structurally: takes
/// user intent (Toggle, Cancel), drives the Rust core's state machine, kicks
/// off recording / transcription, post-processes, injects text. All the
/// semantic decisions (canToggle, status strings, phase transitions) live
/// in Rust — this class is just the orchestrator that wires OS-specific
/// pieces (mic via core, ASR via sherpa, paste via TextInjector) into the
/// state machine. Keep it thin.
/// </summary>
internal sealed class DictationService : IDisposable
{
    private readonly DispatcherQueue _dispatcher;
    private readonly Action<string, IntPtr>? _injectText;

    private Recognizer? _recognizer;
    private readonly Task _initializationTask;

    private IntPtr _targetHwnd;

    /// <summary>Latest download progress, null once load has completed.</summary>
    public DownloadProgress? LastProgress { get; private set; }

    /// <summary>True when the recognizer is loaded and ready to transcribe.</summary>
    public bool IsReady => _recognizer is not null;

    /// <summary>Any exception raised during initial model load, else null.</summary>
    public Exception? LoadError { get; private set; }

    public event EventHandler? ModelLoadProgressChanged;

    public event EventHandler? StateChanged;
    public event EventHandler<string>? LogMessage;

    public DictationService(DispatcherQueue dispatcher, Action<string, IntPtr>? injectText)
    {
        _dispatcher = dispatcher;
        _injectText = injectText;

        // Eagerly load the recognizer at startup. First run downloads ~465 MB;
        // doing it lazily on the first record click would saddle the user with
        // a long silent wait, so we kick it off in the background and surface
        // progress through the UI. The recording path later awaits
        // `_initializationTask` — if the user hits Record before we're done it
        // won't crash, just wait for completion before starting capture.
        _initializationTask = Task.Run(InitializeAsync);
    }

    private async Task InitializeAsync()
    {
        try
        {
            var progress = new Progress<DownloadProgress>(p =>
            {
                LastProgress = p;
                _dispatcher.TryEnqueue(() => ModelLoadProgressChanged?.Invoke(this, EventArgs.Empty));
            });
            SpikeLog.Log("InitializeAsync: starting Recognizer.LoadAsync");
            _recognizer = await Recognizer.LoadAsync(progress).ConfigureAwait(false);
            SpikeLog.Log("InitializeAsync: recognizer ready");
        }
        catch (Exception ex)
        {
            LoadError = ex;
            SpikeLog.Log($"InitializeAsync: FAILED {ex}");
        }
        finally
        {
            LastProgress = null;
            _dispatcher.TryEnqueue(() =>
            {
                ModelLoadProgressChanged?.Invoke(this, EventArgs.Empty);
                StateChanged?.Invoke(this, EventArgs.Empty);
            });
        }
    }

    /// <summary>
    /// User intent: pressed the hotkey / clicked Record.
    /// Let the core decide whether this means start, stop, or ignore.
    /// </summary>
    public void Toggle()
    {
        SpikeLog.Log("Toggle() entered");
        // Capture foreground window BEFORE asking core what to do — if the
        // toggle starts recording, this is the app we'll inject into later.
        // Doing it here (rather than inside BeginAsync) matters for the
        // hotkey path: by the time BeginAsync runs on an async continuation,
        // focus has already wobbled through the hotkey handler.
        var fg = TextInjector.CaptureForegroundWindow();

        var action = Core.RequestToggle();
        SpikeLog.Log($"Toggle() action={action}");
        switch (action)
        {
            case ToggleAction.BeginRecording:
                _targetHwnd = fg;
                _ = SafeRun("BeginAsync", BeginAsync);
                break;
            case ToggleAction.StopRecording:
                _ = SafeRun("StopAsync", StopAsync);
                break;
            case ToggleAction.Ignore:
                break;
        }
        StateChanged?.Invoke(this, EventArgs.Empty);
        SpikeLog.Log("Toggle() exited");
    }

    private static async Task SafeRun(string name, Func<Task> body)
    {
        try { await body().ConfigureAwait(false); }
        catch (Exception ex) { SpikeLog.Log($"{name} FAILED: {ex}"); }
    }

    public void Cancel()
    {
        if (Core.RequestCancel())
        {
            Core.AudioStopCapture();
            StateChanged?.Invoke(this, EventArgs.Empty);
        }
    }

    private async Task BeginAsync()
    {
        SpikeLog.Log("BeginAsync entered");

        // Normal case: recognizer has been ready since shortly after app
        // launch. Rare case: user was very quick on the hotkey and the
        // background load is still in flight — wait for it.
        if (_recognizer is null)
        {
            SpikeLog.Log("BeginAsync: waiting for initialization to finish");
            Core.MarkLoadingModel();
            RaiseStateChanged();
            await _initializationTask.ConfigureAwait(false);
            if (_recognizer is null)
            {
                var msg = LoadError?.Message ?? "model load did not complete";
                SpikeLog.Log($"BeginAsync: initialization unavailable: {msg}");
                Core.DeliverError($"model load failed: {msg}");
                RaiseStateChanged();
                return;
            }
            SpikeLog.Log("BeginAsync: initialization complete, proceeding");
        }

        SpikeLog.Log("BeginAsync: starting audio capture");
        if (!Core.AudioStartCapture(out var err))
        {
            SpikeLog.Log($"BeginAsync: audio start failed: {err}");
            Core.DeliverError(err);
            RaiseStateChanged();
            return;
        }
        Core.MarkCaptureStarted();
        RaiseStateChanged();
        SpikeLog.Log("BeginAsync: capture started");
    }

    private async Task StopAsync()
    {
        SpikeLog.Log("StopAsync entered");
        Core.AudioStopCapture();
        var samples = Core.AudioDrainSamples();
        SpikeLog.Log($"StopAsync drained {samples.Length} samples ({samples.Length / 16_000.0:F2}s)");
        Core.MarkCaptureStopped((ulong)samples.Length);
        RaiseStateChanged();

        if (samples.Length == 0 || _recognizer is null)
        {
            SpikeLog.Log("StopAsync: no samples or no recognizer — bailing");
            RaiseStateChanged();
            return;
        }

        SpikeLog.Log("StopAsync: decoding on thread pool");
        TranscribeResult result;
        try
        {
            result = await Task.Run(() => _recognizer.Transcribe(samples)).ConfigureAwait(false);
        }
        catch (Exception ex)
        {
            SpikeLog.Log($"StopAsync: decode FAILED {ex}");
            Core.DeliverError($"transcription failed: {ex.Message}");
            RaiseStateChanged();
            return;
        }

        var duration = samples.Length / 16_000.0;
        var rtf = duration * 1000 / result.Elapsed.TotalMilliseconds;
        SpikeLog.Log($"StopAsync: decoded {duration:F2}s in {result.Elapsed.TotalMilliseconds:F0} ms ({rtf:F1}x realtime)");
        SpikeLog.Log($"StopAsync: raw=\"{result.RawText}\"");
        Log($"decoded {duration:F2}s of audio in {result.Elapsed.TotalMilliseconds:F0} ms ({rtf:F1}x realtime)");

        var cleaned = Core.ProcessTranscript(result.RawText);
        SpikeLog.Log($"StopAsync: cleaned=\"{cleaned}\"");
        Core.DeliverTranscript(cleaned, confidence: 0.90f);
        RaiseStateChanged();

        if (!string.IsNullOrEmpty(cleaned))
        {
            SpikeLog.Log($"StopAsync: injecting {cleaned.Length} chars into HWND=0x{_targetHwnd.ToInt64():X}");
            _injectText?.Invoke(cleaned, _targetHwnd);
            SpikeLog.Log("StopAsync: injection complete");
        }
    }

    private void Log(string message)
    {
        Debug.WriteLine("[dictation] " + message);
        // Relay log events to the UI via the dispatcher in case subscribers
        // bind them to observable fields. Safe from any thread.
        _dispatcher.TryEnqueue(() => LogMessage?.Invoke(this, message));
    }

    private void RaiseStateChanged()
    {
        _dispatcher.TryEnqueue(() => StateChanged?.Invoke(this, EventArgs.Empty));
    }

    public void Dispose() => _recognizer?.Dispose();
}
