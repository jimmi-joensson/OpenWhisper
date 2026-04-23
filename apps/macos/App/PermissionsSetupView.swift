import SwiftUI

struct PermissionsSetupView: View {
    let coordinator: PermissionsCoordinator

    var body: some View {
        VStack(spacing: 20) {
            Text("Set up OpenWhisper")
                .font(.title2.weight(.semibold))

            switch coordinator.phase {
            case .needsAccessibility:
                step(
                    icon: "keyboard",
                    title: "Step 1 of 2 · Accessibility",
                    body: """
                    OpenWhisper listens for a global hotkey (Right Command by default) \
                    so you can dictate from anywhere, and pastes the transcript into the \
                    focused app when you're done. Both need Accessibility access.
                    """,
                    actionLabel: "Continue",
                    action: coordinator.promptAccessibility
                )
            case .needsAccessibilityRestart:
                step(
                    icon: "arrow.triangle.2.circlepath",
                    title: "Restart required",
                    body: """
                    Accessibility granted. macOS only picks this up on launch, so \
                    OpenWhisper has to restart itself. After the restart we'll ask for \
                    microphone access.
                    """,
                    actionLabel: "Restart OpenWhisper",
                    action: { relaunchOpenWhisper() }
                )
            case .needsMicrophone:
                step(
                    icon: "mic",
                    title: "Step 2 of 2 · Microphone",
                    body: """
                    Needed to capture your speech locally. Nothing is sent anywhere — \
                    audio stays on your Mac and is transcribed by the Parakeet model \
                    running on the Apple Neural Engine.
                    """,
                    actionLabel: "Continue",
                    action: {
                        Task { await coordinator.promptMicrophone() }
                    }
                )
            case .ready:
                EmptyView()
            }
        }
        .padding(40)
        .frame(minWidth: 520)
    }

    private func step(
        icon: String,
        title: String,
        body: String,
        actionLabel: String,
        action: @escaping () -> Void
    ) -> some View {
        VStack(spacing: 16) {
            Image(systemName: icon)
                .font(.system(size: 44, weight: .regular))
                .foregroundStyle(Color.accentColor)
                .padding(.bottom, 4)
            Text(title)
                .font(.headline)
            Text(body)
                .multilineTextAlignment(.center)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: 440)
            Button(actionLabel, action: action)
                .buttonStyle(.borderedProminent)
                .controlSize(.large)
                .padding(.top, 4)
        }
    }
}
