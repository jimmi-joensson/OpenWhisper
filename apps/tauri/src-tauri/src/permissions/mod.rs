//! Boot-time permission prompts. Mirrors
//! `apps/macos/App/PermissionsCoordinator.swift` — fires the system mic
//! dialog proactively rather than waiting for the first record toggle.
//!
//! Sequence (matches Swift):
//!   1. `hotkey::install` prompts Accessibility if not trusted.
//!   2. After AX is trusted (typically post-relaunch on first run), this
//!      module checks the AVCaptureDevice mic auth status and prompts
//!      iff `NotDetermined`.
//!   3. cpal picks up the new mic state on the next `audio_start_capture`.
//!
//! Windows is a no-op — mic on Windows uses AppContainer-style consent
//! handled by the OS at first device open without an explicit request
//! API.

#[cfg(target_os = "macos")]
mod mac;

/// Fire the system mic prompt when warranted. Idempotent — safe to call
/// every boot. Silent no-op when mic is already authorized / denied /
/// restricted, or when AX is not yet trusted (matches Swift sequencing).
pub fn request_microphone() {
    #[cfg(target_os = "macos")]
    mac::request_microphone();
}
