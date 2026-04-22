import SwiftUI

struct ContentView: View {
    @State private var coreMessage: String = "—"
    @State private var coreVersion: String = "—"

    var body: some View {
        VStack(spacing: 16) {
            Text("OpenWhisper")
                .font(.largeTitle.weight(.semibold))

            Text("Rust ↔ Swift smoke test")
                .foregroundStyle(.secondary)

            Divider().frame(maxWidth: 280)

            VStack(alignment: .leading, spacing: 8) {
                LabeledValue(label: "message", value: coreMessage)
                LabeledValue(label: "version", value: coreVersion)
            }
        }
        .padding(40)
        .frame(minWidth: 520, minHeight: 360)
        .task {
            coreMessage = hello_from_rust().toString()
            coreVersion = core_version().toString()
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
                .frame(width: 72, alignment: .trailing)
            Text(value)
                .font(.system(.body, design: .monospaced))
                .textSelection(.enabled)
        }
    }
}

#Preview {
    ContentView()
}
