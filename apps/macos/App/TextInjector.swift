import AppKit
import CoreGraphics

/// Pastes transcribed text into the currently focused app by simulating Cmd+V.
/// Preserves the user's existing clipboard contents by saving them before the
/// paste and restoring them shortly after.
///
/// We do *not* try to use the Accessibility API's `kAXSelectedTextAttribute`
/// set path as a first attempt — it works cleanly in native Cocoa text fields
/// but silently fails in Electron, Chromium web views, Terminal subclasses,
/// and a long tail of other apps. Cmd+V works everywhere Cmd+V normally works,
/// which is what users actually expect.
@MainActor
final class TextInjector {
    /// kVK_ANSI_V from Carbon/HIToolbox. Defined inline so we don't pull in
    /// Carbon just for a single constant.
    private static let vKeyCode: CGKeyCode = 0x09

    /// Restore the user's prior clipboard this long after the paste. 200 ms
    /// is enough for the frontmost app to read the pasteboard contents;
    /// noticeably shorter occasionally clobbers the paste.
    private static let restoreDelayNs: UInt64 = 200_000_000

    /// Inject `text` into whatever app is frontmost. No-ops if OpenWhisper
    /// itself is frontmost (user is looking at the debug window — pasting
    /// into ourselves would be silently confusing).
    func inject(_ text: String) {
        guard !text.isEmpty else { return }
        guard !isOpenWhisperFrontmost() else { return }

        let pasteboard = NSPasteboard.general
        let saved = savePasteboard(pasteboard)

        pasteboard.clearContents()
        pasteboard.setString(text, forType: .string)

        simulatePaste()

        Task {
            try? await Task.sleep(nanoseconds: Self.restoreDelayNs)
            restorePasteboard(pasteboard, saved: saved)
        }
    }

    private func isOpenWhisperFrontmost() -> Bool {
        NSWorkspace.shared.frontmostApplication?.bundleIdentifier
            == Bundle.main.bundleIdentifier
    }

    private func simulatePaste() {
        let source = CGEventSource(stateID: .combinedSessionState)
        let down = CGEvent(keyboardEventSource: source, virtualKey: Self.vKeyCode, keyDown: true)
        let up = CGEvent(keyboardEventSource: source, virtualKey: Self.vKeyCode, keyDown: false)
        down?.flags = .maskCommand
        up?.flags = .maskCommand
        down?.post(tap: .cgAnnotatedSessionEventTap)
        up?.post(tap: .cgAnnotatedSessionEventTap)
    }

    private func savePasteboard(
        _ pasteboard: NSPasteboard
    ) -> [(NSPasteboard.PasteboardType, Data)] {
        let types = pasteboard.types ?? []
        return types.compactMap { type in
            pasteboard.data(forType: type).map { (type, $0) }
        }
    }

    private func restorePasteboard(
        _ pasteboard: NSPasteboard,
        saved: [(NSPasteboard.PasteboardType, Data)]
    ) {
        guard !saved.isEmpty else { return }
        pasteboard.clearContents()
        for (type, data) in saved {
            pasteboard.setData(data, forType: type)
        }
    }
}
