import AppKit
import AVFoundation
import FluidAudio
import Observation
import os

private let log = Logger(subsystem: "com.openwhisper.OpenWhisper", category: "dictation")

/// macOS shell for the dictation pipeline. Owns FluidAudio (Parakeet) and
/// the platform-specific side effects (pill overlay, text injection,
/// record-time polling). The phase machine, status strings, transcript,
/// and error fields live in Rust (`core/src/dictation.rs`) so the Windows
/// shell can reuse the same semantics without re-implementing them.
///
/// Call pattern:
///   1. User intent → `toggle()` / `cancel()`. These ask Rust what to do
///      (`dictation_request_toggle`) and dispatch the mic + STT work here.
///   2. As work progresses, we push events into Rust (`mark_capture_*`,
///      `deliver_transcript`, `deliver_error`).
///   3. After each Rust-side mutation we call `refresh()` to mirror the
///      snapshot into the @Observable properties SwiftUI is watching.
@MainActor
@Observable
final class DictationService {
    /// Mirror of the Rust phase enum. Exists only for Swift call sites
    /// (SwiftUI views, scene bindings) — the source of truth is the u32
    /// value the Rust snapshot returns.
    enum Phase: Equatable {
        case idle, loadingModel, recording, transcribing, done, error

        init(raw: UInt32) {
            switch raw {
            case 1: self = .loadingModel
            case 2: self = .recording
            case 3: self = .transcribing
            case 4: self = .done
            case 5: self = .error
            default: self = .idle
            }
        }
    }

    private enum ToggleAction {
        case ignore, begin, stop

        init(raw: UInt32) {
            switch raw {
            case 1: self = .begin
            case 2: self = .stop
            default: self = .ignore
            }
        }
    }

    private(set) var phase: Phase = .idle
    private(set) var statusMessage: String = "idle — tap Record, speak, tap again"
    private(set) var transcript: String = ""
    private(set) var confidence: String = "—"
    private(set) var errorMessage: String = ""
    private(set) var elapsed: TimeInterval = 0
    private(set) var sampleCount: Int = 0
    private(set) var levelHistory: [Float]

    private static let historyLength = 32

    private let pill: PillWindowController?
    private let injector = TextInjector()
    private var asr: AsrManager?
    private var recordTimer: Timer?

    init(pill: PillWindowController? = nil) {
        self.pill = pill
        self.levelHistory = Array(repeating: 0, count: Self.historyLength)
        refresh()

        // Subscribe to hotkey notifications here — not in a SwiftUI view's
        // .onReceive — so the hotkey keeps working after the main window
        // is closed. Menu-bar agent mode (LSUIElement) leaves the app
        // running with no views mounted; view-scoped observers would
        // silently stop receiving events.
        NotificationCenter.default.addObserver(
            forName: .openWhisperToggleDictation,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.toggle() }
        }
        NotificationCenter.default.addObserver(
            forName: .openWhisperCancelDictation,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.cancel() }
        }
    }

    var isInteractable: Bool {
        switch phase {
        case .idle, .done, .error: return true
        case .loadingModel, .transcribing: return false
        case .recording: return true
        }
    }

    // MARK: - Public API

    func toggle() {
        let action = ToggleAction(raw: dictation_request_toggle())
        refresh()
        switch action {
        case .begin:
            Task { await beginRecordingFlow() }
        case .stop:
            stopAndTranscribe()
        case .ignore:
            break
        }
    }

    /// Abort an in-progress recording. Discards captured audio, does not
    /// transcribe or paste. No-op outside the `.recording` phase — Escape
    /// should only cancel an active session, never disturb other state.
    func cancel() {
        guard dictation_request_cancel() else {
            refresh()
            return
        }
        log.info("cancel requested")
        stopTimer()
        audio_stop_capture()
        _ = audio_drain_samples()  // drop samples on the floor
        pill?.returnToIdleAfter()
        refresh()
    }

    // MARK: - Flow

    private func beginRecordingFlow() async {
        if asr == nil {
            dictation_mark_loading_model()
            refresh()
            let loadStart = Date()
            do {
                let models = try await Task.detached(priority: .userInitiated) {
                    try await AsrModels.downloadAndLoad(version: .v3)
                }.value
                let manager = AsrManager(config: .default)
                try await manager.loadModels(models)
                asr = manager
                let dur = Date().timeIntervalSince(loadStart)
                log.info("asr loaded in \(dur, format: .fixed(precision: 2))s")
            } catch {
                fail("ASR load failed: \(error.localizedDescription)")
                return
            }
        }

        do {
            try audio_start_capture()
            log.info("capture started")
        } catch let rustErr as RustString {
            fail("mic start failed: \(rustErr.toString())")
            return
        } catch {
            fail("mic start failed: \(error.localizedDescription)")
            return
        }

        dictation_mark_capture_started()
        refresh()
        pill?.show(status: .recording)
        startTimer()
    }

    private func stopAndTranscribe() {
        stopTimer()
        audio_stop_capture()

        let drainStart = Date()
        let rustSamples = audio_drain_samples()
        let samples = Array(rustSamples) as [Float]
        let drainDur = Date().timeIntervalSince(drainStart)
        log.info("drained \(samples.count) samples in \(drainDur, format: .fixed(precision: 3))s")

        dictation_mark_capture_stopped(UInt64(samples.count))
        refresh()

        if samples.isEmpty {
            pill?.returnToIdleAfter()
            return
        }

        pill?.show(status: .transcribing)

        guard let manager = asr else {
            fail("ASR not loaded")
            pill?.returnToIdleAfter()
            return
        }

        let samplesCopy = samples
        Task.detached(priority: .userInitiated) {
            let start = Date()
            do {
                let result = try await manager.transcribe(samplesCopy, source: .microphone)
                let dur = Date().timeIntervalSince(start)
                await MainActor.run {
                    let cleaned = process_transcript(result.text).toString()
                    // Transcript text is user content — marked .private so the
                    // unified log store (Console.app / sysdiagnose) redacts it
                    // as `<private>`. Xcode still shows plaintext when
                    // attached via the debugger, so this doesn't hurt dev.
                    log.info("transcribed \(samplesCopy.count) samples in \(dur, format: .fixed(precision: 2))s → raw=\"\(result.text, privacy: .private)\" cleaned=\"\(cleaned, privacy: .private)\" conf=\(result.confidence, format: .fixed(precision: 3))")
                    dictation_deliver_transcript(cleaned, result.confidence)
                    self.refresh()
                    self.injector.inject(cleaned)
                    self.pill?.returnToIdleAfter()
                }
            } catch {
                await MainActor.run {
                    log.error("transcribe failed: \(error.localizedDescription, privacy: .public)")
                    self.fail("transcribe failed: \(error.localizedDescription)")
                    self.pill?.returnToIdleAfter()
                }
            }
        }
    }

    private func fail(_ message: String) {
        dictation_deliver_error(message)
        refresh()
    }

    // MARK: - Rust snapshot mirror

    private func refresh() {
        let snap = dictation_snapshot()
        phase = Phase(raw: snap.phase())
        statusMessage = snap.status_message().toString()
        transcript = snap.transcript().toString()
        confidence = snap.confidence() == 0 ? "—" : String(format: "%.3f", snap.confidence())
        errorMessage = snap.error_message().toString()
        sampleCount = Int(snap.sample_count())
        elapsed = TimeInterval(snap.elapsed_ms()) / 1000.0
    }

    // MARK: - Timer + level meter

    private func startTimer() {
        levelHistory = Array(repeating: 0, count: Self.historyLength)
        pill?.update(levels: Array(repeating: 0, count: pill?.state.levels.count ?? 12))

        recordTimer = Timer.scheduledTimer(withTimeInterval: 0.05, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated { self?.tick() }
        }
    }

    private func stopTimer() {
        recordTimer?.invalidate()
        recordTimer = nil
    }

    private func tick() {
        refresh()
        let level = audio_current_level()
        levelHistory.removeFirst()
        levelHistory.append(level)

        if let pill {
            var pillLevels = pill.state.levels
            pillLevels.removeFirst()
            pillLevels.append(level)
            pill.update(levels: pillLevels)
        }
    }
}
