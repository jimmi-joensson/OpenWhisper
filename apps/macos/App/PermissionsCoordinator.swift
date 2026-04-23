import AppKit
@preconcurrency import ApplicationServices
import AVFoundation
import Observation
import os

private let log = Logger(subsystem: "com.openwhisper.OpenWhisper", category: "permissions")

enum PermissionPhase: Equatable {
    /// Accessibility is not granted. Step 1 of setup.
    case needsAccessibility
    /// Accessibility is now granted but the running process still has stale
    /// TCC state — we can't use it without a fresh process.
    case needsAccessibilityRestart
    /// Accessibility is good; microphone is next.
    case needsMicrophone
    /// Everything granted; the app can dictate.
    case ready
}

/// Single owner of the permission state machine. Prompts are surfaced in a
/// deliberate order — Accessibility first, Microphone second — so the user
/// isn't buried under multiple system dialogs on first launch.
@MainActor
@Observable
final class PermissionsCoordinator {
    private(set) var accessibilityTrusted: Bool
    private(set) var microphoneGranted: Bool
    private(set) var accessibilityGrantedThisSession: Bool = false

    private var pollTask: Task<Void, Never>?

    init() {
        self.accessibilityTrusted = AXIsProcessTrusted()
        self.microphoneGranted = AVCaptureDevice.authorizationStatus(for: .audio) == .authorized
        log.info("init axTrusted=\(self.accessibilityTrusted) micGranted=\(self.microphoneGranted)")
    }

    var phase: PermissionPhase {
        if accessibilityGrantedThisSession { return .needsAccessibilityRestart }
        if !accessibilityTrusted { return .needsAccessibility }
        if !microphoneGranted { return .needsMicrophone }
        return .ready
    }

    /// Fire the system Accessibility prompt. macOS opens System Settings;
    /// this process's trust state is frozen at launch, so once the user
    /// grants we detect the change via poll and move to the restart step.
    func promptAccessibility() {
        let options: NSDictionary = [
            kAXTrustedCheckOptionPrompt.takeUnretainedValue(): true,
        ]
        _ = AXIsProcessTrustedWithOptions(options as CFDictionary)
        startAccessibilityPoll()
    }

    /// Fire the system Microphone prompt. Unlike Accessibility, mic grants
    /// take effect immediately in the running process.
    func promptMicrophone() async {
        let granted = await AVCaptureDevice.requestAccess(for: .audio)
        microphoneGranted = granted
        log.info("microphone prompt result granted=\(granted)")
    }

    private func startAccessibilityPoll() {
        pollTask?.cancel()
        pollTask = Task { @MainActor [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(1))
                guard let self else { return }
                let trusted = AXIsProcessTrusted()
                if trusted, !self.accessibilityTrusted {
                    log.info("accessibility granted during this session — restart required")
                    self.accessibilityTrusted = true
                    self.accessibilityGrantedThisSession = true
                    return
                }
            }
        }
    }
}
