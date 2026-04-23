import AppKit
import AVFoundation
import FluidAudio
import Observation
import os

private let log = Logger(subsystem: "com.openwhisper.OpenWhisper", category: "dictation")

/// Single owner of the dictation pipeline: mic start/stop, model loading,
/// transcription, pill overlay updates, and text injection. Consumed by
/// ContentView (and any future surfaces — settings, menu bar, etc.) through
/// observable state. Replaces the mess of @State variables that had
/// accumulated in ContentView.
@MainActor
@Observable
final class DictationService {
    /// High-level lifecycle. `.idle` → `.loadingModel` (first use only) →
    /// `.recording` → `.transcribing` → `.done` (or `.error`).
    enum Phase: Equatable {
        case idle
        case loadingModel
        case recording
        case transcribing
        case done
        case error(String)
    }

    private(set) var phase: Phase = .idle
    private(set) var statusMessage: String = "idle — tap Record, speak, tap again"
    private(set) var transcript: String = ""
    private(set) var confidence: String = "—"
    private(set) var elapsed: TimeInterval = 0
    private(set) var sampleCount: Int = 0
    private(set) var levelHistory: [Float]

    private static let historyLength = 32

    private let pill: PillWindowController?
    private let injector = TextInjector()
    let processor = TranscriptProcessor()
    private var asr: AsrManager?
    private var recordTimer: Timer?
    private var recordStart: Date?

    init(pill: PillWindowController? = nil) {
        self.pill = pill
        self.levelHistory = Array(repeating: 0, count: Self.historyLength)
    }

    var isInteractable: Bool {
        switch phase {
        case .idle, .done, .error: return true
        case .loadingModel, .recording, .transcribing: return phase == .recording
        }
    }

    /// True only when a user tap should toggle the recording state.
    /// Blocks double-taps during model load and transcription.
    private var canToggle: Bool {
        switch phase {
        case .idle, .done, .error, .recording: return true
        case .loadingModel, .transcribing: return false
        }
    }

    // MARK: - Public API

    func toggle() {
        guard canToggle else { return }
        if case .recording = phase {
            stopAndTranscribe()
        } else {
            startRecording()
        }
    }

    /// Abort an in-progress recording. Discards the captured audio, does
    /// not transcribe or paste. No-op if we aren't recording — Escape
    /// should only cancel an active session, never disturb other state.
    func cancel() {
        guard case .recording = phase else { return }
        log.info("cancel requested")
        stopTimer()
        audio_stop_capture()
        _ = audio_drain_samples()  // drop samples on the floor
        transcript = ""
        confidence = "—"
        sampleCount = 0
        elapsed = 0
        statusMessage = "cancelled"
        phase = .idle
        pill?.hideAfter()
    }

    // MARK: - Flow

    private func startRecording() {
        transcript = ""
        confidence = "—"
        elapsed = 0
        sampleCount = 0
        statusMessage = "preparing…"

        Task { await beginRecordingFlow() }
    }

    private func beginRecordingFlow() async {
        if asr == nil {
            phase = .loadingModel
            statusMessage = "loading Parakeet model (first run ~500 MB)…"
            let loadStart = Date()
            do {
                let models = try await Task.detached(priority: .userInitiated) {
                    try await AsrModels.downloadAndLoad(version: .v2)
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

        phase = .recording
        statusMessage = "recording — tap again to stop"
        pill?.show(status: .recording)
        startTimer()
    }

    private func stopAndTranscribe() {
        stopTimer()
        audio_stop_capture()
        phase = .transcribing
        statusMessage = "draining mic buffer…"
        pill?.show(status: .transcribing)

        let drainStart = Date()
        let rustSamples = audio_drain_samples()
        let samples = Array(rustSamples) as [Float]
        sampleCount = samples.count
        let drainDur = Date().timeIntervalSince(drainStart)
        log.info("drained \(samples.count) samples in \(drainDur, format: .fixed(precision: 3))s")

        if samples.isEmpty {
            statusMessage = "no audio captured"
            phase = .done
            pill?.hideAfter()
            return
        }

        statusMessage = "transcribing on ANE…"

        guard let manager = asr else {
            fail("ASR not loaded")
            pill?.hideAfter()
            return
        }

        let samplesCopy = samples
        Task.detached(priority: .userInitiated) {
            let start = Date()
            do {
                let result = try await manager.transcribe(samplesCopy, source: .microphone)
                let dur = Date().timeIntervalSince(start)
                await MainActor.run {
                    let cleaned = self.processor.process(result.text)
                    log.info("transcribed \(samplesCopy.count) samples in \(dur, format: .fixed(precision: 2))s → raw=\"\(result.text, privacy: .public)\" cleaned=\"\(cleaned, privacy: .public)\" conf=\(result.confidence, format: .fixed(precision: 3))")
                    self.transcript = cleaned
                    self.confidence = String(format: "%.3f", result.confidence)
                    self.injector.inject(cleaned)
                    self.statusMessage = "done — pasted to focused app"
                    self.phase = .done
                    self.pill?.hideAfter()
                }
            } catch {
                await MainActor.run {
                    log.error("transcribe failed: \(error.localizedDescription, privacy: .public)")
                    self.fail("transcribe failed: \(error.localizedDescription)")
                    self.pill?.hideAfter()
                }
            }
        }
    }

    private func fail(_ message: String) {
        statusMessage = message
        phase = .error(message)
    }

    // MARK: - Timer + level meter

    private func startTimer() {
        recordStart = Date()
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
        if let start = recordStart {
            elapsed = Date().timeIntervalSince(start)
        }
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
