import AppKit
import SwiftUI

@main
struct OpenWhisperApp: App {
    @State private var hotkey = HotkeyService()
    @State private var pill: PillWindowController
    @State private var permissions = PermissionsCoordinator()
    @State private var dictation: DictationService

    init() {
        // Enforce single-instance before any Scene mounts. During a TCC-driven
        // relaunch (Relaunch.swift spawns a new instance, then terminates),
        // both processes briefly own a MenuBarExtra → two mic icons in the
        // bar. Terminating the predecessor here closes that window.
        Self.terminatePriorInstances()

        let pill = PillWindowController()
        let dictation = DictationService(pill: pill)
        self._pill = State(wrappedValue: pill)
        self._dictation = State(wrappedValue: dictation)
    }

    private static func terminatePriorInstances() {
        guard let bundleID = Bundle.main.bundleIdentifier else { return }
        let me = NSRunningApplication.current
        let others = NSRunningApplication
            .runningApplications(withBundleIdentifier: bundleID)
            .filter { $0.processIdentifier != me.processIdentifier }

        guard !others.isEmpty else { return }

        for other in others { other.terminate() }

        // Block briefly until predecessors actually exit. Worst case we wait
        // 2s and proceed anyway — falls back to the old (visible) overlap
        // rather than hanging the user.
        let deadline = Date().addingTimeInterval(2.0)
        while Date() < deadline {
            let stillAlive = NSRunningApplication
                .runningApplications(withBundleIdentifier: bundleID)
                .contains { $0.processIdentifier != me.processIdentifier }
            if !stillAlive { return }
            usleep(50_000) // 50 ms
        }
    }

    var body: some Scene {
        Window("OpenWhisper", id: "main") {
            ContentView()
                .environment(\.hotkey, hotkey)
                .environment(\.pill, pill)
                .environment(\.permissions, permissions)
                .environment(\.dictation, dictation)
        }
        .defaultSize(width: 580, height: 540)

        MenuBarExtra {
            MenuBarContent(dictation: dictation)
        } label: {
            // SF Symbol auto-tints to match the menu bar (template image).
            Image(systemName: menuBarSymbol(for: dictation.phase))
        }
        .menuBarExtraStyle(.menu)
    }

    private func menuBarSymbol(for phase: DictationService.Phase) -> String {
        switch phase {
        case .recording: return "waveform"
        case .transcribing: return "waveform.and.mic"
        default: return "mic.fill"
        }
    }
}

/// Contents of the menu-bar dropdown. Lives inside the MenuBarExtra scene
/// so `openWindow` resolves via the SwiftUI environment without any
/// AppKit bridging.
private struct MenuBarContent: View {
    @Environment(\.openWindow) private var openWindow
    @Bindable var dictation: DictationService

    var body: some View {
        Button("Open OpenWhisper") {
            openWindow(id: "main")
            NSApp.activate(ignoringOtherApps: true)
        }

        Divider()

        Button(dictationLabel) {
            dictation.toggle()
        }
        .disabled(!dictation.isInteractable)

        Divider()

        Button("Quit OpenWhisper") {
            NSApp.terminate(nil)
        }
        .keyboardShortcut("q")
    }

    private var dictationLabel: String {
        switch dictation.phase {
        case .idle, .done, .error: return "Start Dictation"
        case .loadingModel: return "Loading model…"
        case .recording: return "Stop Dictation"
        case .transcribing: return "Transcribing…"
        }
    }
}

private struct HotkeyServiceKey: EnvironmentKey {
    static let defaultValue: HotkeyService? = nil
}

private struct PillControllerKey: EnvironmentKey {
    static let defaultValue: PillWindowController? = nil
}

private struct PermissionsCoordinatorKey: EnvironmentKey {
    static let defaultValue: PermissionsCoordinator? = nil
}

private struct DictationServiceKey: EnvironmentKey {
    static let defaultValue: DictationService? = nil
}

extension EnvironmentValues {
    var hotkey: HotkeyService? {
        get { self[HotkeyServiceKey.self] }
        set { self[HotkeyServiceKey.self] = newValue }
    }
    var pill: PillWindowController? {
        get { self[PillControllerKey.self] }
        set { self[PillControllerKey.self] = newValue }
    }
    var permissions: PermissionsCoordinator? {
        get { self[PermissionsCoordinatorKey.self] }
        set { self[PermissionsCoordinatorKey.self] = newValue }
    }
    var dictation: DictationService? {
        get { self[DictationServiceKey.self] }
        set { self[DictationServiceKey.self] = newValue }
    }
}
