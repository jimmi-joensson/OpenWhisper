import SwiftUI
import AVFoundation
import FluidAudio
import os

private let log = Logger(subsystem: "com.openwhisper.OpenWhisper", category: "dictation")


struct ContentView: View {
    @Environment(\.hotkey) private var hotkey
    @Environment(\.pill) private var pill
    @Environment(\.permissions) private var permissions

    @State private var coreMessage: String = "—"
    @State private var coreVersion: String = "—"

    @State private var status: String = "idle — tap Record, speak, tap again"
    @State private var transcript: String = ""
    @State private var confidence: String = "—"
    @State private var elapsed: TimeInterval = 0
    @State private var sampleCount: Int = 0

    @State private var isRecording = false
    @State private var isTranscribing = false
    @State private var recordTimer: Timer?
    @State private var levelHistory: [Float] = Array(repeating: 0, count: 32)

    // Keep the loaded ASR across captures so we only pay the load cost once.
    @State private var asr: AsrManager?
    @State private var injector = TextInjector()

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("OpenWhisper")
                .font(.largeTitle.weight(.semibold))
                .frame(maxWidth: .infinity, alignment: .center)

            GroupBox("Rust ↔ Swift FFI") {
                VStack(alignment: .leading, spacing: 4) {
                    LabeledValue(label: "message", value: coreMessage)
                    LabeledValue(label: "version", value: coreVersion)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(8)
            }

            Text("tap Right Command to toggle dictation from anywhere")
                .font(.callout)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .center)

            if permissions?.accessibilityGrantedThisSession == true {
                HStack(spacing: 12) {
                    Image(systemName: "arrow.triangle.2.circlepath")
                    Text("Accessibility granted. Restart OpenWhisper to activate the hotkey.")
                    Spacer()
                    Button("Restart") { relaunchOpenWhisper() }
                        .buttonStyle(.borderedProminent)
                }
                .padding(10)
                .background(.blue.opacity(0.15), in: RoundedRectangle(cornerRadius: 8))
                .overlay(
                    RoundedRectangle(cornerRadius: 8)
                        .strokeBorder(.blue.opacity(0.35), lineWidth: 1)
                )
            }

            GroupBox("Permissions & hotkey debug") {
                VStack(alignment: .leading, spacing: 4) {
                    LabeledValue(
                        label: "accessibility",
                        value: permissions?.accessibilityTrusted == true ? "granted" : "not granted"
                    )
                    LabeledValue(
                        label: "microphone",
                        value: permissions?.microphoneGranted == true ? "granted" : "not granted"
                    )
                    if let hotkey {
                        LabeledValue(label: "tap", value: hotkey.tapStatus)
                        LabeledValue(label: "events seen", value: "\(hotkey.eventCount)")
                        if let ev = hotkey.lastEvent {
                            LabeledValue(
                                label: "last event",
                                value: "\(ev.type) keyCode=\(ev.keyCode) flags=\(ev.flagsHex) rCmd=\(ev.rightCommandDown ? "↓" : "·")"
                            )
                        } else {
                            LabeledValue(label: "last event", value: "none yet — tap any key")
                        }
                        Button("Retry tap install") { hotkey.retryInstall() }
                            .controlSize(.small)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(8)
            }

            GroupBox("Dictation (mic → Rust core → Parakeet)") {
                VStack(alignment: .leading, spacing: 10) {
                    LabeledValue(label: "status", value: status)
                    LabeledValue(label: "elapsed", value: String(format: "%.1f s", elapsed))
                    LabeledValue(label: "samples", value: sampleCount == 0 ? "—" : "\(sampleCount) @ 16 kHz")
                    LabeledValue(label: "confidence", value: confidence)

                    LevelMeter(levels: levelHistory, active: isRecording)
                        .frame(height: 36)
                        .padding(.vertical, 4)

                    VStack(alignment: .leading, spacing: 4) {
                        Text("transcript:").foregroundStyle(.tertiary)
                        ScrollView {
                            Text(transcript.isEmpty ? "—" : transcript)
                                .font(.system(.body, design: .monospaced))
                                .textSelection(.enabled)
                                .frame(maxWidth: .infinity, alignment: .leading)
                        }
                        .frame(minHeight: 60, maxHeight: 140)
                        .padding(8)
                        .background(.black.opacity(0.2), in: RoundedRectangle(cornerRadius: 6))
                    }

                    Button(action: toggle) {
                        Label(
                            buttonLabel,
                            systemImage: isRecording ? "stop.circle.fill" : "mic.circle.fill"
                        )
                    }
                    .disabled(isTranscribing)
                    .controlSize(.large)
                    .buttonStyle(.borderedProminent)
                    .tint(isRecording ? .red : .accentColor)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(8)
            }
        }
        .padding(20)
        .frame(minWidth: 580, minHeight: 540)
        .task {
            coreMessage = hello_from_rust().toString()
            coreVersion = core_version().toString()
        }
        .onReceive(NotificationCenter.default.publisher(for: .openWhisperToggleDictation)) { _ in
            toggle()
        }
    }

    private var buttonLabel: String {
        if isTranscribing { return "Transcribing…" }
        if isRecording { return "Stop & transcribe" }
        return "Record"
    }

    // MARK: - Actions

    private func toggle() {
        if isRecording {
            stopAndTranscribe()
        } else {
            startRecording()
        }
    }

    private func startRecording() {
        transcript = ""
        confidence = "—"
        elapsed = 0
        sampleCount = 0
        status = "preparing…"

        Task {
            if asr == nil {
                status = "loading Parakeet model (first run ~500 MB)…"
                let loadStart = Date()
                do {
                    // Run the model load off the main actor — it involves
                    // heavy CoreML compilation and file IO.
                    let models = try await Task.detached(priority: .userInitiated) {
                        try await AsrModels.downloadAndLoad(version: .v2)
                    }.value
                    let manager = AsrManager(config: .default)
                    try await manager.loadModels(models)
                    asr = manager
                    let dur = Date().timeIntervalSince(loadStart)
                    log.info("asr loaded in \(dur, format: .fixed(precision: 2))s")
                } catch {
                    status = "ASR load failed: \(error.localizedDescription)"
                    log.error("asr load failed: \(error.localizedDescription, privacy: .public)")
                    return
                }
            }

            do {
                try audio_start_capture()
                log.info("capture started")
            } catch let rustErr as RustString {
                status = "mic start failed: \(rustErr.toString())"
                return
            } catch {
                status = "mic start failed: \(error.localizedDescription)"
                return
            }

            isRecording = true
            status = "recording — tap again to stop"
            pill?.show(status: .recording)
            startTimer()
        }
    }

    private func stopAndTranscribe() {
        stopTimer()
        audio_stop_capture()
        isRecording = false
        isTranscribing = true
        status = "draining mic buffer…"
        pill?.show(status: .transcribing)

        let drainStart = Date()
        let rustSamples = audio_drain_samples()
        let samples = Array(rustSamples) as [Float]
        sampleCount = samples.count
        let drainDur = Date().timeIntervalSince(drainStart)
        log.info("drained \(samples.count) samples in \(drainDur, format: .fixed(precision: 3))s")

        if samples.isEmpty {
            status = "no audio captured"
            isTranscribing = false
            pill?.hideAfter()
            return
        }

        status = "transcribing on ANE…"

        guard let manager = asr else {
            status = "error: ASR not loaded"
            isTranscribing = false
            pill?.hideAfter()
            return
        }

        // Run transcribe detached so heavy CoreML work never sits on the
        // main actor. Hop back to main only to publish results.
        let samplesCopy = samples
        Task.detached(priority: .userInitiated) {
            let start = Date()
            do {
                let result = try await manager.transcribe(samplesCopy, source: .microphone)
                let dur = Date().timeIntervalSince(start)
                await MainActor.run {
                    log.info("transcribed \(samplesCopy.count) samples in \(dur, format: .fixed(precision: 2))s → \"\(result.text, privacy: .public)\" conf=\(result.confidence, format: .fixed(precision: 3))")
                    transcript = result.text
                    confidence = String(format: "%.3f", result.confidence)
                    injector.inject(result.text)
                    status = "done — pasted to focused app"
                    isTranscribing = false
                    pill?.hideAfter()
                }
            } catch {
                await MainActor.run {
                    log.error("transcribe failed: \(error.localizedDescription, privacy: .public)")
                    status = "transcribe failed: \(error.localizedDescription)"
                    isTranscribing = false
                    pill?.hideAfter()
                }
            }
        }
    }

    // MARK: - Timer

    private func startTimer() {
        let start = Date()
        levelHistory = Array(repeating: 0, count: levelHistory.count)
        // The pill has a shorter ring than the debug view; reset it too so
        // the bars start flat when a new session begins.
        pill?.update(levels: Array(repeating: 0, count: pill?.state.levels.count ?? 12))
        recordTimer = Timer.scheduledTimer(withTimeInterval: 0.05, repeats: true) { _ in
            elapsed = Date().timeIntervalSince(start)
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

    private func stopTimer() {
        recordTimer?.invalidate()
        recordTimer = nil
        levelHistory = Array(repeating: 0, count: levelHistory.count)
    }
}

private struct LabeledValue: View {
    let label: String
    let value: String

    var body: some View {
        HStack(spacing: 8) {
            Text("\(label):")
                .foregroundStyle(.tertiary)
                .frame(width: 92, alignment: .trailing)
            Text(value)
                .font(.system(.body, design: .monospaced))
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}

#Preview {
    ContentView()
}
