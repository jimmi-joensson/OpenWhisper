import AppKit
import IOKit.hid

extension Notification.Name {
    /// Posted when the user taps the activation hotkey (default: Right Command).
    static let openWhisperToggleDictation = Notification.Name("com.openwhisper.toggleDictation")
}

/// Watches keyboard events system-wide and fires `openWhisperToggleDictation`
/// when the user taps **Right Command** with no other key in between. Left
/// Command is ignored, and holding Right Command as a chord modifier (e.g.
/// Cmd+Q) does not trigger a toggle.
///
/// Requires "Input Monitoring" permission (TCC). We call `IOHIDRequestAccess`
/// on install so the system prompts the user the first time; subsequent
/// launches pick up the grant automatically.
@MainActor
final class HotkeyService {
    /// Device-dependent bit inside `NSEvent.ModifierFlags.rawValue` for the
    /// Right Command key. Distinguishing left vs right is not part of the
    /// public `ModifierFlags` API, but the underlying CGEvent flags carry
    /// the same bit, documented as `NX_DEVICERCMDKEYMASK` in IOKit headers.
    private static let rightCommandMask: UInt = 0x0010

    private var globalMonitor: Any?
    private var localMonitor: Any?
    private var rightCommandDown = false
    private var otherKeyPressedWhileHeld = false

    init() {
        requestAccessIfNeeded()
        install()
    }

    private func requestAccessIfNeeded() {
        let status = IOHIDCheckAccess(kIOHIDRequestTypeListenEvent)
        if status != kIOHIDAccessTypeGranted {
            // Fires the system TCC prompt. The user may need to relaunch the
            // app after granting for the global monitor to start receiving
            // events; we don't try to fix that up automatically in MVP.
            _ = IOHIDRequestAccess(kIOHIDRequestTypeListenEvent)
        }
    }

    private func install() {
        let mask: NSEvent.EventTypeMask = [.flagsChanged, .keyDown]
        globalMonitor = NSEvent.addGlobalMonitorForEvents(matching: mask) { [weak self] event in
            Task { @MainActor [weak self] in self?.handle(event) }
        }
        localMonitor = NSEvent.addLocalMonitorForEvents(matching: mask) { [weak self] event in
            self?.handle(event)
            return event
        }
    }

    private func handle(_ event: NSEvent) {
        switch event.type {
        case .flagsChanged:
            let flags = event.modifierFlags.rawValue
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
