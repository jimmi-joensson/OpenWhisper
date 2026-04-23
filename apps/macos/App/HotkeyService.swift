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
/// pasting transcribed text into the focused app. One permission, one prompt,
/// matches Superwhisper's UX. The alternative (NSEvent global monitor) would
/// require a *separate* Input Monitoring grant, so the user would see two
/// prompts for what is conceptually one capability.
@MainActor
final class HotkeyService {
    /// Device-dependent bit for Right Command inside `CGEventFlags.rawValue`,
    /// corresponding to `NX_DEVICERCMDKEYMASK` in `IOKit/hidsystem/ev_keymap.h`.
    /// Distinguishing left vs right is not part of the public CGEventFlags API,
    /// but the underlying bit is stable across macOS versions.
    private static let rightCommandMask: UInt64 = 0x0010

    private var tap: CFMachPort?
    private var runLoopSource: CFRunLoopSource?
    private var rightCommandDown = false
    private var otherKeyPressedWhileHeld = false

    init() {
        requestAccessibilityIfNeeded()
        installTap()
    }

    private func requestAccessibilityIfNeeded() {
        // Triggers the system Accessibility prompt if the app isn't trusted.
        // On first launch the user is taken to System Settings to grant access;
        // they typically need to relaunch the app for the tap to start firing.
        let options: NSDictionary = [
            kAXTrustedCheckOptionPrompt.takeUnretainedValue(): true,
        ]
        _ = AXIsProcessTrustedWithOptions(options as CFDictionary)
    }

    private func installTap() {
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
            // Usually means Accessibility hasn't been granted yet. The user
            // will see the system prompt and can relaunch after approving.
            return
        }

        let source = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, tap, 0)
        CFRunLoopAddSource(CFRunLoopGetMain(), source, .commonModes)
        CGEvent.tapEnable(tap: tap, enable: true)

        self.tap = tap
        self.runLoopSource = source
    }

    // C callback: fires on the main run loop (we attach it there). We unpack
    // state to primitives synchronously and call into the MainActor-isolated
    // handler via `assumeIsolated` so Swift 6 strict concurrency is happy.
    private static let eventCallback: CGEventTapCallBack = { _, type, event, refcon in
        guard let refcon else { return Unmanaged.passUnretained(event) }
        let service = Unmanaged<HotkeyService>.fromOpaque(refcon).takeUnretainedValue()

        // Extract primitives before crossing the MainActor boundary so the
        // non-Sendable CGEvent is never captured by the isolated closure.
        let flags = event.flags.rawValue

        MainActor.assumeIsolated {
            service.handle(type: type, flags: flags)
        }

        // Pass every event through unchanged — we observe, we don't swallow.
        return Unmanaged.passUnretained(event)
    }

    private func handle(type: CGEventType, flags: UInt64) {
        switch type {
        case .flagsChanged:
            let nowDown = (flags & Self.rightCommandMask) != 0
            if nowDown, !rightCommandDown {
                rightCommandDown = true
                otherKeyPressedWhileHeld = false
            } else if !nowDown, rightCommandDown {
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
}
