using System.Diagnostics;
using Microsoft.UI.Dispatching;

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
    private readonly Action<string>? _injectText;

    // Recognizer is lazily loaded on first toggle — model download + warmup
    // is expensive (~seconds) so we don't block app startup.
    private Recognizer? _recognizer;
    private Task<Recognizer>? _recognizerLoad;

    public event EventHandler? StateChanged;
    public event EventHandler<string>? LogMessage;

    public DictationService(DispatcherQueue dispatcher, Action<string>? injectText)
    {
        _dispatcher = dispatcher;
        _injectText = injectText;
    }

    /// <summary>
    /// User intent: pressed the hotkey / clicked Record.
    /// Let the core decide whether this means start, stop, or ignore.
    /// </summary>
    public void Toggle()
    {
        var action = Core.RequestToggle();
        switch (action)
        {
            case ToggleAction.BeginRecording:
                _ = BeginAsync();
                break;
            case ToggleAction.StopRecording:
                _ = StopAsync();
                break;
            case ToggleAction.Ignore:
                // Core rejected the toggle — busy state, already transcribing, etc.
                break;
        }
        StateChanged?.Invoke(this, EventArgs.Empty);
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
        // Load the recognizer on first use. Further toggles reuse the warm instance.
        if (_recognizer is null)
        {
            Core.MarkLoadingModel();
            RaiseStateChanged();
            try
            {
                _recognizerLoad ??= Recognizer.LoadAsync(
                    progress: new Progress<DownloadProgress>(p =>
                    {
                        var pct = p.PercentComplete is double v ? $"{v:F0}%" : $"{p.BytesReceived / 1_048_576.0:F1} MB";
                        Log($"downloading Parakeet v3 — {pct}");
                    }));
                _recognizer = await _recognizerLoad.ConfigureAwait(false);
                Log($"recognizer ready (core {Core.Version})");
            }
            catch (Exception ex)
            {
                Core.DeliverError($"model load failed: {ex.Message}");
                RaiseStateChanged();
                return;
            }
        }

        if (!Core.AudioStartCapture(out var err))
        {
            Core.DeliverError(err);
            RaiseStateChanged();
            return;
        }
        Core.MarkCaptureStarted();
        RaiseStateChanged();
    }

    private async Task StopAsync()
    {
        Core.AudioStopCapture();
        var samples = Core.AudioDrainSamples();
        Core.MarkCaptureStopped((ulong)samples.Length);
        RaiseStateChanged();

        if (samples.Length == 0 || _recognizer is null)
        {
            // Core already transitioned to Done via the zero-sample path; nothing to decode.
            RaiseStateChanged();
            return;
        }

        // Decode off the UI thread; the recognizer itself is thread-safe for
        // a single concurrent call, which is all we issue.
        TranscribeResult result;
        try
        {
            result = await Task.Run(() => _recognizer.Transcribe(samples)).ConfigureAwait(false);
        }
        catch (Exception ex)
        {
            Core.DeliverError($"transcription failed: {ex.Message}");
            RaiseStateChanged();
            return;
        }

        var duration = samples.Length / 16_000.0;
        var rtf = duration * 1000 / result.Elapsed.TotalMilliseconds;
        Log($"decoded {duration:F2}s of audio in {result.Elapsed.TotalMilliseconds:F0} ms ({rtf:F1}x realtime)");

        // Two-step: process then deliver, same as macOS DictationService.
        var cleaned = Core.ProcessTranscript(result.RawText);
        Core.DeliverTranscript(cleaned, confidence: 0.90f);
        RaiseStateChanged();

        // Inject into the currently focused app. This IS the product — the
        // UI window is incidental. Runs on whatever thread we're on; the
        // injector is fire-and-forget.
        if (!string.IsNullOrEmpty(cleaned))
        {
            _injectText?.Invoke(cleaned);
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
