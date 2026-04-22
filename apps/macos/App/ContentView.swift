import SwiftUI
import FluidAudio

struct ContentView: View {
    @State private var coreMessage: String = "—"
    @State private var coreVersion: String = "—"

    @State private var status: String = "idle"
    @State private var transcript: String = ""
    @State private var confidence: String = "—"
    @State private var isRunning = false

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

            GroupBox("Parakeet (FluidAudio) smoke test") {
                VStack(alignment: .leading, spacing: 10) {
                    LabeledValue(label: "status", value: status)
                    LabeledValue(
                        label: "confidence",
                        value: confidence
                    )
                    VStack(alignment: .leading, spacing: 4) {
                        Text("transcript:")
                            .foregroundStyle(.tertiary)
                        ScrollView {
                            Text(transcript.isEmpty ? "—" : transcript)
                                .font(.system(.body, design: .monospaced))
                                .textSelection(.enabled)
                                .frame(maxWidth: .infinity, alignment: .leading)
                        }
                        .frame(minHeight: 60, maxHeight: 120)
                        .padding(8)
                        .background(.black.opacity(0.2), in: RoundedRectangle(cornerRadius: 6))
                    }

                    Button(action: runSmokeTest) {
                        Label(
                            isRunning ? "Running…" : "Transcribe bundled sample",
                            systemImage: "waveform"
                        )
                    }
                    .disabled(isRunning)
                    .controlSize(.large)
                    .buttonStyle(.borderedProminent)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(8)
            }
        }
        .padding(20)
        .frame(minWidth: 560, minHeight: 460)
        .task {
            coreMessage = hello_from_rust().toString()
            coreVersion = core_version().toString()
        }
    }

    private func runSmokeTest() {
        isRunning = true
        transcript = ""
        confidence = "—"
        status = "loading models (first run downloads ~500 MB)…"

        Task.detached {
            do {
                let models = try await AsrModels.downloadAndLoad(version: .v2)
                await MainActor.run { status = "configuring ASR…" }

                let asr = AsrManager(config: .default)
                try await asr.loadModels(models)

                guard let sampleURL = Bundle.main.url(
                    forResource: "smoke-test",
                    withExtension: "wav"
                ) else {
                    throw SmokeTestError.missingSample
                }

                await MainActor.run { status = "transcribing on ANE…" }
                let result = try await asr.transcribe(sampleURL, source: .system)

                await MainActor.run {
                    transcript = result.text
                    confidence = String(format: "%.3f", result.confidence)
                    status = "done"
                    isRunning = false
                }
            } catch {
                await MainActor.run {
                    status = "error: \(error.localizedDescription)"
                    isRunning = false
                }
            }
        }
    }
}

private enum SmokeTestError: LocalizedError {
    case missingSample

    var errorDescription: String? {
        switch self {
        case .missingSample:
            return "smoke-test.wav not found in app bundle"
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
