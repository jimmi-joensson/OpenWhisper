import AppKit
@preconcurrency import ApplicationServices
import CoreGraphics

extension Notification.Name {
    /// Posted when the user taps the activation hotkey (default: Right Command).
    static let openWhisperToggleDictation = Notification.Name("com.openwhisper.toggleDictation")
}

/// Watches keyboard events system-wide via a CGEventTap and fires
/// `openWhisperToggleDictation` when the user taps **Right Command** with no
/// other key in between. Left Command is ignored, and holding Right Command
/// as a chord modifier (e.g. Cmd+Q) does not trigger a toggle.
///
/// Uses Accessibility permission — the same grant OpenWhisper needs for
/// pasting transcribed text into the focused app. One permission, one prompt.
@MainActor
@Observable
final class HotkeyService {
    /// Device-dependent bit for Right Command inside `CGEventFlags.rawValue`,
    /// corresponding to `NX_DEVICERCMDKEYMASK` in `IOKit/hidsystem/ev_keymap.h`.
    private static let rightCommandMask: UInt64 = 0x0010

    // Debug/diagnostic state — surfaced in ContentView so you can see at a
    // glance whether Accessibility is granted and whether events are flowing.
    private(set) var isAccessibilityTrusted = false
    private(set) var tapStatus: String = "not installed"
    private(set) var lastEvent: DebugEvent?
    private(set) var eventCount: Int = 0

    /// True when the user has granted Accessibility *after* we tried to
    /// install the tap, meaning we need a fresh process to pick up the new
    /// TCC state. Drives the in-app "Restart" banner.
    private(set) var needsRelaunch = false

    private var accessibilityPollTask: Task<Void, Never>?

    struct DebugEvent {
        let type: String
        let flagsHex: String
        let keyCode: UInt16
        let rightCommandDown: Bool
    }

    private var tap: CFMachPort?
    private var runLoopSource: CFRunLoopSource?
    private var rightCommandDown = false
    private var otherKeyPressedWhileHeld = false

    init() {
        requestAccessibilityIfNeeded()
        installTap()
    }

    func retryInstall() {
        if tap == nil {
            installTap()
        }
    }

    private func requestAccessibilityIfNeeded() {
        let options: NSDictionary = [
            kAXTrustedCheckOptionPrompt.takeUnretainedValue(): true,
        ]
        _ = AXIsProcessTrustedWithOptions(options as CFDictionary)
    }

    private func installTap() {
        isAccessibilityTrusted = AXIsProcessTrusted()

        let mask: CGEventMask =
            (1 << CGEventType.flagsChanged.rawValue) |
            (1 << CGEventType.keyDown.rawValue)

        let refcon = Unmanaged.passUnretained(self).toOpaque()

        guard let tap = CGEvent.tapCreate(
            tap: .cgSessionEventTap,
            place: .headInsertEventTap,
            options: .defaultTap,
            eventsOfInterest: mask,
            callback: Self.eventCallback,
            userInfo: refcon
        ) else {
            tapStatus = isAccessibilityTrusted
                ? "tapCreate returned nil (signature changed after grant? re-toggle in System Settings)"
                : "waiting for Accessibility permission"
            startAccessibilityPoll()
            return
        }

        let source = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, tap, 0)
        CFRunLoopAddSource(CFRunLoopGetMain(), source, .commonModes)
        CGEvent.tapEnable(tap: tap, enable: true)

        self.tap = tap
        self.runLoopSource = source
        tapStatus = "installed"
        accessibilityPollTask?.cancel()
        accessibilityPollTask = nil
    }

    /// Poll Accessibility status so that when the user grants it in
    /// System Settings while we're already running, we can show an in-app
    /// "Restart" banner instead of silently doing nothing.
    private func startAccessibilityPoll() {
        guard accessibilityPollTask == nil else { return }
        accessibilityPollTask = Task { @MainActor [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(1))
                guard let self else { return }
                let trusted = AXIsProcessTrusted()
                if trusted, !self.isAccessibilityTrusted {
                    self.isAccessibilityTrusted = true
                    self.needsRelaunch = true
                    return
                }
            }
        }
    }

    private static let eventCallback: CGEventTapCallBack = { _, type, event, refcon in
        guard let refcon else { return Unmanaged.passUnretained(event) }
        let service = Unmanaged<HotkeyService>.fromOpaque(refcon).takeUnretainedValue()

        // Extract primitives before crossing the MainActor boundary so the
        // non-Sendable CGEvent is never captured by the isolated closure.
        let flags = event.flags.rawValue
        let keyCode = UInt16(event.getIntegerValueField(.keyboardEventKeycode))

        MainActor.assumeIsolated {
            service.handle(type: type, flags: flags, keyCode: keyCode)
        }

        // Pass every event through unchanged — we observe, we don't swallow.
        return Unmanaged.passUnretained(event)
    }

    private func handle(type: CGEventType, flags: UInt64, keyCode: UInt16) {
        let rightDown = (flags & Self.rightCommandMask) != 0
        eventCount += 1
        lastEvent = DebugEvent(
            type: Self.describe(type),
            flagsHex: String(format: "0x%06llx", flags),
            keyCode: keyCode,
            rightCommandDown: rightDown
        )

        switch type {
        case .flagsChanged:
            if rightDown, !rightCommandDown {
                rightCommandDown = true
                otherKeyPressedWhileHeld = false
            } else if !rightDown, rightCommandDown {
                rightCommandDown = false
                if !otherKeyPressedWhileHeld {
                    NotificationCenter.default.post(
                        name: .openWhisperToggleDictation,
                        object: nil
                    )
                }
            }
        case .keyDown:
            if rightCommandDown {
                otherKeyPressedWhileHeld = true
            }
        default:
            break
        }
    }

    private static func describe(_ type: CGEventType) -> String {
        switch type {
        case .flagsChanged: return "flagsChanged"
        case .keyDown: return "keyDown"
        case .keyUp: return "keyUp"
        default: return "other(\(type.rawValue))"
        }
    }
}
