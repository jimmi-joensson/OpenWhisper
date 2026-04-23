import AppKit

/// Start a fresh instance of this app bundle, then terminate the current
/// process once the new one is launching. Used after the user grants
/// Accessibility (or any other TCC category) — the running process's trust
/// state is frozen at launch, so a restart is the only way to pick up the
/// grant without the signature dance.
@MainActor
func relaunchOpenWhisper() {
    let url = Bundle.main.bundleURL
    let config = NSWorkspace.OpenConfiguration()
    config.createsNewApplicationInstance = true

    NSWorkspace.shared.openApplication(at: url, configuration: config) { _, _ in
        // Small delay so the new instance is fully launching before we exit.
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) {
            NSApp.terminate(nil)
        }
    }
}
