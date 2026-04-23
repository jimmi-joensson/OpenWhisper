import SwiftUI

@main
struct OpenWhisperApp: App {
    @State private var hotkey = HotkeyService()
    @State private var pill = PillWindowController()

    var body: some Scene {
        WindowGroup("OpenWhisper") {
            ContentView()
                .environment(\.hotkey, hotkey)
                .environment(\.pill, pill)
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

extension EnvironmentValues {
    var hotkey: HotkeyService? {
        get { self[HotkeyServiceKey.self] }
        set { self[HotkeyServiceKey.self] = newValue }
    }
    var pill: PillWindowController? {
        get { self[PillControllerKey.self] }
        set { self[PillControllerKey.self] = newValue }
    }
}
