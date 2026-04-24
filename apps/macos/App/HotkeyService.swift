import AppKit
@preconcurrency import ApplicationServices
import CoreGraphics
import os

private let log = Logger(subsystem: "com.openwhisper.OpenWhisper", category: "hotkey")

extension Notification.Name {
    /// Posted when the user taps the activation hotkey (default: Right Command).
    static let openWhisperToggleDictation = Notification.Name("com.openwhisper.toggleDictation")

    /// Posted when the user presses Escape. Receivers decide whether to act
    /// on it (e.g. DictationService cancels only when recording).
    static let openWhisperCancelDictation = Notification.Name("com.openwhisper.cancelDictation")
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

    /// kVK_Escape.
    private static let escapeKeyCode: UInt16 = 0x35

    // Debug/diagnostic state — surfaced in ContentView so you can see at a
    // glance whether Accessibility is granted and whether events are flowing.
    private(set) var isAccessibilityTrusted = false
    private(set) var tapStatus: String = "not installed"
    private(set) var lastEvent: DebugEvent?
    private(set) var eventCount: Int = 0

    struct DebugEvent {
        let type: String
        let flagsHex: String
        let keyCode: UInt16
        let rightCommandDown: Bool
    }

    private var tap: CFMachPort?
    private var runLoopSource: CFRunLoopSource?
    private var tapThread: Thread?
    private var rightCommandDown = false
    private var otherKeyPressedWhileHeld = false
    private var watchdogTimer: Timer?

    init() {
        let pid = ProcessInfo.processInfo.processIdentifier
        let trusted = AXIsProcessTrusted()
        log.info("HotkeyService.init pid=\(pid) trusted=\(trusted)")
        // No permission prompt here — PermissionsCoordinator owns the
        // step-by-step permission flow. We just try to install the tap; if
        // Accessibility is not granted, tapCreate returns nil and we stay
        // in a benign "waiting" state until a Restart/retry fixes it.
        installTap()
        log.info("HotkeyService.init done trusted=\(self.isAccessibilityTrusted) tapStatus=\(self.tapStatus, privacy: .public)")
    }

    func retryInstall() {
        if tap == nil {
            installTap()
        }
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
            return
        }

        self.tap = tap

        // Run the tap on a dedicated thread instead of the main run loop.
        // CGEventTap callbacks have a ~2s budget; if main is stalled (big
        // SwiftUI renders, another app pushing heavy events, a video call
        // hogging UI time), our callback misses the deadline and macOS
        // fires `tapDisabledByTimeout`. Isolating the tap on its own
        // high-QoS thread means main-thread busy periods can't kill it.
        // Events hop back to the main actor for handling.
        let source = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, tap, 0)
        self.runLoopSource = source

        let thread = Thread { [weak self] in
            guard let tap = self?.tap, let source = self?.runLoopSource else { return }
            let runLoop = CFRunLoopGetCurrent()
            CFRunLoopAddSource(runLoop, source, .commonModes)
            CGEvent.tapEnable(tap: tap, enable: true)
            CFRunLoopRun()
        }
        thread.name = "com.openwhisper.hotkey-tap"
        thread.qualityOfService = .userInteractive
        thread.start()
        self.tapThread = thread

        tapStatus = "installed"
        startWatchdog()
    }

    /// Backstop: verify the tap is still live every 5s. With the tap on
    /// its own thread, `tapDisabledByTimeout` from main-thread stalls is
    /// no longer a realistic cause of death. But sleep/wake, TCC
    /// revocation, and other edge cases can still silently kill the tap
    /// without delivering a `tapDisabledBy*` event, so the watchdog stays.
    private func startWatchdog() {
        watchdogTimer?.invalidate()
        watchdogTimer = Timer.scheduledTimer(withTimeInterval: 5.0, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated { self?.checkTapHealth() }
        }
    }

    private func checkTapHealth() {
        guard let tap = self.tap else { return }
        if !CGEvent.tapIsEnabled(tap: tap) {
            log.warning("event tap went stale — re-enabling")
            CGEvent.tapEnable(tap: tap, enable: true)
            tapStatus = "re-enabled by watchdog"
        }
    }

    private static let eventCallback: CGEventTapCallBack = { _, type, event, refcon in
        guard let refcon else { return Unmanaged.passUnretained(event) }
        let service = Unmanaged<HotkeyService>.fromOpaque(refcon).takeUnretainedValue()

        // Runs on the dedicated tap thread, NOT the main thread. Never
        // call `MainActor.assumeIsolated` here — it would crash. Hop back
        // to main via DispatchQueue.main.async for any shared-state work.

        // macOS fires one of these synthetic events when it disables the
        // tap (timeout, user-input policing, internal heuristics). Re-enable
        // right here on the tap thread — no main hop needed, and we want
        // the tap alive as fast as possible regardless of main-thread state.
        if type == .tapDisabledByTimeout || type == .tapDisabledByUserInput {
            if let tap = service.tap {
                CGEvent.tapEnable(tap: tap, enable: true)
            }
            DispatchQueue.main.async {
                MainActor.assumeIsolated { service.handleTapDisabled(reason: type) }
            }
            return Unmanaged.passUnretained(event)
        }

        // Extract CGEvent primitives before marshaling — CGEvent isn't
        // Sendable and must not be captured across threads.
        let flags = event.flags.rawValue
        let keyCode = UInt16(event.getIntegerValueField(.keyboardEventKeycode))

        DispatchQueue.main.async {
            MainActor.assumeIsolated {
                service.handle(type: type, flags: flags, keyCode: keyCode)
            }
        }

        // Pass every event through unchanged — we observe, we don't swallow.
        return Unmanaged.passUnretained(event)
    }

    private func handleTapDisabled(reason: CGEventType) {
        let label = reason == .tapDisabledByTimeout ? "timeout" : "user-input"
        log.warning("event tap disabled by system (\(label, privacy: .public)) — re-enabling")
        tapStatus = "re-enabled after \(label)"
        if let tap = self.tap {
            CGEvent.tapEnable(tap: tap, enable: true)
        }
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
            if keyCode == Self.escapeKeyCode {
                NotificationCenter.default.post(
                    name: .openWhisperCancelDictation,
                    object: nil
                )
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
