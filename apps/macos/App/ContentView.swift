import SwiftUI

struct ContentView: View {
    @Environment(\.hotkey) private var hotkey
    @Environment(\.permissions) private var permissions
    @Environment(\.dictation) private var dictation

    @State private var coreMessage: String = "—"
    @State private var coreVersion: String = "—"

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

            Text("Right Command to toggle · Escape to cancel while recording")
                .font(.callout)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .center)

            if permissions?.accessibilityGrantedThisSession == true {
                restartBanner
            }

            debugPanel

            dictationPanel
        }
        .padding(20)
        .frame(minWidth: 580, minHeight: 540)
        .task {
            coreMessage = hello_from_rust().toString()
            coreVersion = core_version().toString()
        }
        .onReceive(NotificationCenter.default.publisher(for: .openWhisperToggleDictation)) { _ in
            dictation?.toggle()
        }
        .onReceive(NotificationCenter.default.publisher(for: .openWhisperCancelDictation)) { _ in
            dictation?.cancel()
        }
    }

    private var restartBanner: some View {
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

    private var debugPanel: some View {
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
    }

    @ViewBuilder
    private var dictationPanel: some View {
        if let dictation {
            GroupBox("Dictation (mic → Rust core → Parakeet)") {
                VStack(alignment: .leading, spacing: 10) {
                    LabeledValue(label: "status", value: dictation.statusMessage)
                    LabeledValue(label: "elapsed", value: String(format: "%.1f s", dictation.elapsed))
                    LabeledValue(
                        label: "samples",
                        value: dictation.sampleCount == 0 ? "—" : "\(dictation.sampleCount) @ 16 kHz"
                    )
                    LabeledValue(label: "confidence", value: dictation.confidence)

                    LevelMeter(levels: dictation.levelHistory, active: dictation.phase == .recording)
                        .frame(height: 36)
                        .padding(.vertical, 4)

                    VStack(alignment: .leading, spacing: 4) {
                        Text("transcript:").foregroundStyle(.tertiary)
                        ScrollView {
                            Text(dictation.transcript.isEmpty ? "—" : dictation.transcript)
                                .font(.system(.body, design: .monospaced))
                                .textSelection(.enabled)
                                .frame(maxWidth: .infinity, alignment: .leading)
                        }
                        .frame(minHeight: 60, maxHeight: 140)
                        .padding(8)
                        .background(.black.opacity(0.2), in: RoundedRectangle(cornerRadius: 6))
                    }

                    Button(action: { dictation.toggle() }) {
                        Label(buttonLabel(for: dictation.phase),
                              systemImage: dictation.phase == .recording ? "stop.circle.fill" : "mic.circle.fill")
                    }
                    .disabled(isButtonDisabled(for: dictation.phase))
                    .controlSize(.large)
                    .buttonStyle(.borderedProminent)
                    .tint(dictation.phase == .recording ? .red : .accentColor)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(8)
            }
        }
    }

    private func buttonLabel(for phase: DictationService.Phase) -> String {
        switch phase {
        case .loadingModel: return "Loading…"
        case .transcribing: return "Transcribing…"
        case .recording: return "Stop & transcribe"
        case .idle, .done, .error: return "Record"
        }
    }

    private func isButtonDisabled(for phase: DictationService.Phase) -> Bool {
        switch phase {
        case .loadingModel, .transcribing: return true
        default: return false
        }
    }
}

struct LabeledValue: View {
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
