import SwiftUI

@main
struct OpenWhisperApp: App {
    @State private var hotkey = HotkeyService()

    var body: some Scene {
        WindowGroup("OpenWhisper") {
            ContentView()
                .environment(\.hotkey, hotkey)
        }
        .defaultSize(width: 580, height: 540)
    }
}

private struct HotkeyServiceKey: EnvironmentKey {
    static let defaultValue: HotkeyService? = nil
}

extension EnvironmentValues {
    var hotkey: HotkeyService? {
        get { self[HotkeyServiceKey.self] }
        set { self[HotkeyServiceKey.self] = newValue }
    }
}
