import SwiftUI

@main
struct OpenWhisperApp: App {
    @State private var hotkey = HotkeyService()
    @State private var pill = PillWindowController()
    @State private var permissions = PermissionsCoordinator()

    var body: some Scene {
        WindowGroup("OpenWhisper") {
            ContentView()
                .environment(\.hotkey, hotkey)
                .environment(\.pill, pill)
                .environment(\.permissions, permissions)
        }
        .defaultSize(width: 580, height: 540)
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
}
