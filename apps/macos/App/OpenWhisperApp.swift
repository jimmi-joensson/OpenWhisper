import AppKit
import Observation
import SwiftUI

@main
struct OpenWhisperApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate

    @State private var hotkey = HotkeyService()
    @State private var pill: PillWindowController
    @State private var permissions = PermissionsCoordinator()
    @State private var dictation: DictationService

    init() {
        // Enforce single-instance before any Scene mounts. During a TCC-driven
        // relaunch (Relaunch.swift spawns a new instance, then terminates),
        // both processes briefly own a status item → two mic icons in the bar.
        Self.terminatePriorInstances()

        let pill = PillWindowController()
        let dictation = DictationService(pill: pill)
        self._pill = State(wrappedValue: pill)
        self._dictation = State(wrappedValue: dictation)

        // Hand the dictation service to the AppDelegate via a shared bridge.
        // App.init runs before applicationDidFinishLaunching, so the delegate
        // sees this when it sets up the NSStatusItem.
        AppBridge.dictation = dictation
    }

    private static func terminatePriorInstances() {
        guard let bundleID = Bundle.main.bundleIdentifier else { return }
        let me = NSRunningApplication.current
        let others = NSRunningApplication
            .runningApplications(withBundleIdentifier: bundleID)
            .filter { $0.processIdentifier != me.processIdentifier }

        guard !others.isEmpty else { return }

        for other in others { other.terminate() }

        let deadline = Date().addingTimeInterval(2.0)
        while Date() < deadline {
            let stillAlive = NSRunningApplication
                .runningApplications(withBundleIdentifier: bundleID)
                .contains { $0.processIdentifier != me.processIdentifier }
            if !stillAlive { return }
            usleep(50_000)
        }
    }

    var body: some Scene {
        Window(Bundle.main.appDisplayName, id: "main") {
            ContentView()
                .environment(\.hotkey, hotkey)
                .environment(\.pill, pill)
                .environment(\.permissions, permissions)
                .environment(\.dictation, dictation)
                .modifier(CaptureOpenWindow())
        }
        .defaultSize(width: 580, height: 540)
    }
}

/// Stash SwiftUI's `openWindow` action in `AppBridge` so the AppKit
/// status-bar menu can reopen the main window after a close. The window
/// auto-opens once at launch (Window scene), so onAppear fires at least
/// once and the closure is captured before the user can possibly close it.
private struct CaptureOpenWindow: ViewModifier {
    @Environment(\.openWindow) private var openWindow

    func body(content: Content) -> some View {
        content.onAppear {
            AppBridge.openMainWindow = { openWindow(id: "main") }
        }
    }
}

/// Shared bridge between the SwiftUI App graph and the AppKit AppDelegate.
/// Set in `App.init` (dictation) and `ContentView.onAppear` (openMainWindow).
@MainActor
enum AppBridge {
    static var dictation: DictationService?
    static var openMainWindow: (() -> Void)?
}

// MARK: - AppDelegate

/// Owns the NSStatusItem and its NSMenu directly, sidestepping SwiftUI's
/// MenuBarExtra (FB13683957: label doesn't rerender on @Observable changes).
/// We get full control over the icon image swap in response to dictation
/// phase changes via withObservationTracking.
@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate, NSMenuDelegate {
    private var statusItem: NSStatusItem?
    private let menu = NSMenu()
    private weak var dictation: DictationService?

    func applicationDidFinishLaunching(_ notification: Notification) {
        guard let dictation = AppBridge.dictation else {
            assertionFailure("AppBridge.dictation must be set in App.init before delegate runs")
            return
        }
        self.dictation = dictation

        let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        item.button?.toolTip = Bundle.main.appDisplayName
        item.menu = menu

        menu.delegate = self
        menu.autoenablesItems = false

        statusItem = item

        refreshIcon()
        observePhase()
    }

    // MARK: - Reactive icon

    /// Re-arms itself on every change because withObservationTracking only
    /// fires once per registration. Idiomatic AppKit consumer of @Observable.
    private func observePhase() {
        guard let dictation else { return }
        withObservationTracking {
            _ = dictation.phase
        } onChange: { [weak self] in
            DispatchQueue.main.async {
                self?.refreshIcon()
                self?.observePhase()
            }
        }
    }

    private func refreshIcon() {
        guard let dictation else { return }
        statusItem?.button?.image = StatusIconRenderer.render(phase: dictation.phase)
    }

    // MARK: - Menu

    func menuWillOpen(_ menu: NSMenu) {
        rebuildMenu()
    }

    private func rebuildMenu() {
        menu.removeAllItems()

        let openItem = NSMenuItem(title: "Open \(Bundle.main.appDisplayName)", action: #selector(openMain), keyEquivalent: "")
        openItem.target = self
        openItem.isEnabled = true
        menu.addItem(openItem)

        menu.addItem(.separator())

        let dictationItem = NSMenuItem(title: dictationItemTitle(), action: #selector(toggleDictation), keyEquivalent: "")
        dictationItem.target = self
        dictationItem.isEnabled = dictation?.isInteractable ?? false
        menu.addItem(dictationItem)

        menu.addItem(.separator())

        let quitItem = NSMenuItem(title: "Quit \(Bundle.main.appDisplayName)", action: #selector(quit), keyEquivalent: "q")
        quitItem.target = self
        quitItem.isEnabled = true
        menu.addItem(quitItem)
    }

    private func dictationItemTitle() -> String {
        switch dictation?.phase {
        case .recording: return "Stop Dictation"
        case .loadingModel: return "Loading model…"
        case .transcribing: return "Transcribing…"
        case .none, .some(.idle), .some(.done), .some(.error): return "Start Dictation"
        }
    }

    // MARK: - Actions

    @objc private func openMain() {
        NSApp.activate(ignoringOtherApps: true)
        if let window = NSApp.windows.first(where: { $0.canBecomeKey }) {
            window.makeKeyAndOrderFront(nil)
            return
        }
        AppBridge.openMainWindow?()
    }

    @objc private func toggleDictation() {
        dictation?.toggle()
    }

    @objc private func quit() {
        NSApp.terminate(nil)
    }
}

// MARK: - Status icon rendering

/// Builds the menu-bar NSImage from the same rect coordinates as the
/// source SVGs (OpenWhisper_Default.svg / OpenWhisper_Recording.svg).
/// Idle = template image so AppKit auto-tints for dark/light + highlight.
/// Recording = composite (mic + orange badge) drawn explicitly with
/// NSColor.labelColor so the mic still adapts to dark/light, and the
/// badge stays vivid orange — at the cost of menu-highlight inversion
/// on the recording-state icon.
@MainActor
enum StatusIconRenderer {
    private static let viewBox: CGFloat = 792
    private static let iconSize: CGFloat = 18

    private static let micRects: [CGRect] = [
        CGRect(x: 204, y: 188, width: 64, height: 64),
        CGRect(x: 204, y: 284, width: 64, height: 64),
        CGRect(x: 204, y: 380, width: 64, height: 64),
        CGRect(x: 204, y: 476, width: 64, height: 64),
        CGRect(x: 204, y: 700, width: 64, height: 64),
        CGRect(x: 268, y: 28, width: 64, height: 64),
        CGRect(x: 268, y: 92, width: 256, height: 64),
        CGRect(x: 268, y: 188, width: 64, height: 64),
        CGRect(x: 268, y: 284, width: 64, height: 64),
        CGRect(x: 268, y: 380, width: 64, height: 64),
        CGRect(x: 268, y: 476, width: 256, height: 64),
        CGRect(x: 268, y: 700, width: 256, height: 64),
        CGRect(x: 364, y: 28, width: 64, height: 64),
        CGRect(x: 364, y: 156, width: 64, height: 320),
        CGRect(x: 364, y: 572, width: 64, height: 64),
        CGRect(x: 364, y: 636, width: 64, height: 64),
        CGRect(x: 460, y: 28, width: 64, height: 64),
        CGRect(x: 460, y: 188, width: 64, height: 64),
        CGRect(x: 460, y: 284, width: 64, height: 64),
        CGRect(x: 460, y: 380, width: 64, height: 64),
        CGRect(x: 524, y: 92, width: 64, height: 64),
        CGRect(x: 524, y: 188, width: 64, height: 64),
        CGRect(x: 524, y: 284, width: 64, height: 64),
        CGRect(x: 524, y: 380, width: 64, height: 64),
        CGRect(x: 524, y: 476, width: 64, height: 64),
        CGRect(x: 524, y: 700, width: 64, height: 64),
    ]

    private static let badgeRects: [CGRect] = [
        CGRect(x: 524, y: 92, width: 64, height: 64),
        CGRect(x: 524, y: 188, width: 64, height: 64),
        CGRect(x: 620, y: 92, width: 64, height: 64),
        CGRect(x: 620, y: 188, width: 64, height: 64),
    ]

    private static let badgeColor = NSColor(red: 0.88, green: 0.44, blue: 0, alpha: 1) // #E07000

    static func render(phase: DictationService.Phase) -> NSImage {
        if phase == .recording {
            return recordingImage
        }
        return idleImage
    }

    private static let idleImage: NSImage = {
        let img = NSImage(size: NSSize(width: iconSize, height: iconSize), flipped: false) { rect in
            let scale = rect.width / viewBox
            NSColor.black.setFill()
            for r in micRects { drawRect(r, scale: scale, in: rect).fill() }
            return true
        }
        img.isTemplate = true
        return img
    }()

    private static let recordingImage: NSImage = {
        let img = NSImage(size: NSSize(width: iconSize, height: iconSize), flipped: false) { rect in
            let scale = rect.width / viewBox
            // Mic body in label color so it still reads on light + dark bars.
            NSColor.labelColor.setFill()
            for r in micRects { drawRect(r, scale: scale, in: rect).fill() }
            // Badge rects on top, vivid orange, NOT template-tinted.
            badgeColor.setFill()
            for r in badgeRects { drawRect(r, scale: scale, in: rect).fill() }
            return true
        }
        // NOT template: we want the orange to stay orange. Mic body uses
        // labelColor which still resolves to the current appearance.
        img.isTemplate = false
        return img
    }()

    /// Convert source SVG-coord rect to target NSImage rect (Y-flipped).
    private static func drawRect(_ r: CGRect, scale: CGFloat, in bounds: NSRect) -> NSRect {
        NSRect(
            x: r.minX * scale,
            y: bounds.height - (r.minY + r.height) * scale,
            width: r.width * scale,
            height: r.height * scale
        )
    }
}

// MARK: - Environment keys

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

extension Bundle {
    /// Display name for the running build — "OpenWhisper" in Release, "OpenWhisper Dev" in Debug.
    /// Driven by per-config PRODUCT_NAME, which CFBundleDisplayName/CFBundleName inherit.
    var appDisplayName: String {
        (object(forInfoDictionaryKey: "CFBundleDisplayName") as? String)
            ?? (object(forInfoDictionaryKey: kCFBundleNameKey as String) as? String)
            ?? "OpenWhisper"
    }
}
