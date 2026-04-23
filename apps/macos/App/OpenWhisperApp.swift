import SwiftUI

@main
struct OpenWhisperApp: App {
    @State private var hotkey = HotkeyService()
    @State private var pill: PillWindowController
    @State private var permissions = PermissionsCoordinator()
    @State private var dictation: DictationService

    init() {
        let pill = PillWindowController()
        self._pill = State(wrappedValue: pill)
        self._dictation = State(wrappedValue: DictationService(pill: pill))
    }

    var body: some Scene {
        WindowGroup("OpenWhisper") {
            ContentView()
                .environment(\.hotkey, hotkey)
                .environment(\.pill, pill)
                .environment(\.permissions, permissions)
                .environment(\.dictation, dictation)
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
