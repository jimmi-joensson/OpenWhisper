import SwiftUI

@main
struct OpenWhisperApp: App {
    var body: some Scene {
        WindowGroup("OpenWhisper") {
            ContentView()
        }
        .defaultSize(width: 520, height: 360)
        .windowResizability(.contentSize)
    }
}
