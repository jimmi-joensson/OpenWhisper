import AppKit
import os

private let log = Logger(subsystem: "com.openwhisper.OpenWhisper", category: "relaunch")

/// Spawn a fresh instance of this app bundle via `/usr/bin/open -n -a`, then
/// terminate the current process. Using `open` rather than
/// NSWorkspace.openApplication because the latter sometimes *activates* an
/// existing instance instead of spawning a new one — which defeats the point
/// of relaunching to pick up a new TCC grant.
@MainActor
func relaunchOpenWhisper() {
    let path = Bundle.main.bundlePath
    let task = Process()
    task.executableURL = URL(fileURLWithPath: "/usr/bin/open")
    task.arguments = ["-n", "-a", path]

    do {
        try task.run()
        log.info("relaunch spawned new instance for \(path, privacy: .public)")
    } catch {
        log.error("relaunch failed to spawn: \(error.localizedDescription, privacy: .public)")
        return
    }

    // Let the new instance make it past its initial registration before we
    // exit. 0.5s is comfortable without feeling laggy.
    DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
        NSApp.terminate(nil)
    }
}
