import SwiftUI
import AVFoundation
import FluidAudio

private enum MicPermission: String {
    case checking
    case granted
    case denied
    case awaitingPrompt = "awaiting prompt"

    var display: String {
        switch self {
        case .denied: return "denied — enable in System Settings"
        default: return rawValue
        }
    }
}

struct ContentView: View {
    @Environment(\.hotkey) private var hotkey
    @Environment(\.pill) private var pill

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
    @State private var micPermission: MicPermission = .checking

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

            if hotkey?.needsRelaunch == true {
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
                    LabeledValue(label: "microphone", value: micPermission.display)
                    if let hotkey {
                        LabeledValue(
                            label: "accessibility",
                            value: hotkey.isAccessibilityTrusted ? "granted" : "not granted — grant + relaunch"
                        )
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
            await resolveMicPermission()
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
                do {
                    let models = try await AsrModels.downloadAndLoad(version: .v2)
                    let manager = AsrManager(config: .default)
                    try await manager.loadModels(models)
                    asr = manager
                } catch {
                    status = "ASR load failed: \(error.localizedDescription)"
                    return
                }
            }

            do {
                try audio_start_capture()
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

        let rustSamples = audio_drain_samples()
        let samples = Array(rustSamples) as [Float]
        sampleCount = samples.count

        if samples.isEmpty {
            status = "no audio captured"
            isTranscribing = false
            pill?.hideAfter()
            return
        }

        status = "transcribing on ANE…"

        Task {
            guard let manager = asr else {
                status = "error: ASR not loaded"
                isTranscribing = false
                return
            }
            do {
                let result = try await manager.transcribe(samples, source: .microphone)
                transcript = result.text
                confidence = String(format: "%.3f", result.confidence)
                injector.inject(result.text)
                status = "done — pasted to focused app"
            } catch {
                status = "transcribe failed: \(error.localizedDescription)"
            }
            isTranscribing = false
            pill?.hideAfter()
        }
    }

    // MARK: - Permissions

    /// Resolve microphone access at app launch so the system prompt fires
    /// here — not halfway into the user's first utterance. Safe to call
    /// repeatedly; a prior decision short-circuits immediately.
    private func resolveMicPermission() async {
        switch AVCaptureDevice.authorizationStatus(for: .audio) {
        case .authorized:
            micPermission = .granted
        case .notDetermined:
            micPermission = .awaitingPrompt
            let granted = await AVCaptureDevice.requestAccess(for: .audio)
            micPermission = granted ? .granted : .denied
        case .denied, .restricted:
            micPermission = .denied
        @unknown default:
            micPermission = .denied
        }
    }

    // MARK: - Timer

    private func startTimer() {
        let start = Date()
        levelHistory = Array(repeating: 0, count: levelHistory.count)
        // The pill has a shorter ring than the debug view; reset it too so
        // the bars start flat when a new session begins.
        pill?.update(levels: Array(repeating: 0, count: pill?.state.levels.count ?? 24))
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
