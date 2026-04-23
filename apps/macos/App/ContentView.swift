import SwiftUI
import FluidAudio

struct ContentView: View {
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
            startTimer()
        }
    }

    private func stopAndTranscribe() {
        stopTimer()
        audio_stop_capture()
        isRecording = false
        isTranscribing = true
        status = "draining mic buffer…"

        let rustSamples = audio_drain_samples()
        let samples = Array(rustSamples) as [Float]
        sampleCount = samples.count

        if samples.isEmpty {
            status = "no audio captured"
            isTranscribing = false
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
                status = "done"
            } catch {
                status = "transcribe failed: \(error.localizedDescription)"
            }
            isTranscribing = false
        }
    }

    // MARK: - Timer

    private func startTimer() {
        let start = Date()
        levelHistory = Array(repeating: 0, count: levelHistory.count)
        recordTimer = Timer.scheduledTimer(withTimeInterval: 0.05, repeats: true) { _ in
            elapsed = Date().timeIntervalSince(start)
            // Shift left and push the new sample.
            let level = audio_current_level()
            levelHistory.removeFirst()
            levelHistory.append(level)
        }
    }

    private func stopTimer() {
        recordTimer?.invalidate()
        recordTimer = nil
        levelHistory = Array(repeating: 0, count: levelHistory.count)
    }
}

private struct LevelMeter: View {
    let levels: [Float]
    let active: Bool

    // RMS on mic usually lives around 0.02–0.3 during speech. Scale so
    // conversational input roughly fills the meter without clipping.
    private static let gain: Float = 4.0

    var body: some View {
        GeometryReader { geo in
            let barSpacing: CGFloat = 2
            let barCount = CGFloat(levels.count)
            let barWidth = max(1, (geo.size.width - barSpacing * (barCount - 1)) / barCount)

            HStack(alignment: .center, spacing: barSpacing) {
                ForEach(Array(levels.enumerated()), id: \.offset) { _, level in
                    let scaled = CGFloat(min(1.0, level * LevelMeter.gain))
                    let h = max(2, scaled * geo.size.height)
                    RoundedRectangle(cornerRadius: 1.5)
                        .fill(active ? Color.accentColor : Color.secondary.opacity(0.35))
                        .frame(width: barWidth, height: h)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .center)
        }
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
