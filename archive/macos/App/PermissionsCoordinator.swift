import AppKit
@preconcurrency import ApplicationServices
import AVFoundation
import Observation
import os

private let log = Logger(subsystem: "com.openwhisper.OpenWhisper", category: "permissions")

/// Single owner of the permission state. Fires system prompts in a
/// deliberate order — Accessibility first, Microphone second — so the user
/// never faces both TCC dialogs at once. No in-app setup UI is shown; the
/// main window loads normally and the system handles the prompts.
@MainActor
@Observable
final class PermissionsCoordinator {
    private(set) var accessibilityTrusted: Bool
    private(set) var microphoneGranted: Bool
    /// True when Accessibility was granted *during this running process*,
    /// meaning the new trust state won't take effect until relaunch. Drives
    /// the inline Restart banner.
    private(set) var accessibilityGrantedThisSession: Bool = false

    private var pollTask: Task<Void, Never>?

    init() {
        self.accessibilityTrusted = AXIsProcessTrusted()
        self.microphoneGranted = AVCaptureDevice.authorizationStatus(for: .audio) == .authorized
        log.info("init axTrusted=\(self.accessibilityTrusted) micGranted=\(self.microphoneGranted)")

        resumeFlow()
    }

    /// Fire the next needed system prompt (if any). Called on init, and
    /// intended for re-entry after a relaunch.
    private func resumeFlow() {
        if !accessibilityTrusted {
            log.info("resumeFlow → prompting Accessibility")
            promptAccessibility()
        } else if !microphoneGranted {
            log.info("resumeFlow → prompting Microphone")
            Task { await promptMicrophone() }
        }
    }

    private func promptAccessibility() {
        let options: NSDictionary = [
            kAXTrustedCheckOptionPrompt.takeUnretainedValue(): true,
        ]
        _ = AXIsProcessTrustedWithOptions(options as CFDictionary)
        startAccessibilityPoll()
    }

    private func promptMicrophone() async {
        let granted = await AVCaptureDevice.requestAccess(for: .audio)
        microphoneGranted = granted
        log.info("mic prompt granted=\(granted)")
    }

    private func startAccessibilityPoll() {
        pollTask?.cancel()
        pollTask = Task { @MainActor [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(1))
                guard let self else { return }
                let trusted = AXIsProcessTrusted()
                if trusted, !self.accessibilityTrusted {
                    log.info("accessibility granted this session — restart required")
                    self.accessibilityTrusted = true
                    self.accessibilityGrantedThisSession = true
                    return
                }
            }
        }
    }
}
