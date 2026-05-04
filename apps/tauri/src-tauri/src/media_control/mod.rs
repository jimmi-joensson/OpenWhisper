//! Pause/resume of other apps' audio playback while OpenWhisper is
//! recording. The phase observer in `spawn_dictation_emitter` calls
//! `pause_now` on PHASE_RECORDING entry (when the
//! `behavior.pause_audio_during_dictation` cache is on) and
//! `resume_now` on exit. Real impls live in `mac.rs` (MediaRemote +
//! CoreAudio) and `windows.rs` (SMTC + IAudioEndpointVolume).

pub trait MediaController: Send + Sync {
    /// Returns true if the call paused something (so the observer
    /// knows to call `resume_now` on PHASE_RECORDING exit).
    fn pause_now(&self) -> bool;

    fn resume_now(&self);
}

#[cfg(target_os = "macos")]
mod mac;
#[cfg(target_os = "macos")]
pub use mac::MacMediaController as PlatformMediaController;

/// Cross-platform diagnostic surface for the most recent `pause_now`
/// call. Mac populates this when AppleScript paused nothing because of
/// a TCC Automation denial — the silent-failure case that prompted
/// this code. Other platforms return `None` (Windows SMTC has its own
/// failure modes but they don't manifest as "paused nothing despite
/// playback" the way Mac does, so no surface needed yet).
#[derive(Debug, Clone, serde::Serialize)]
pub struct PauseDiagnostic {
    /// Stable machine tag — UI switches on this to render the right
    /// banner. `not_authorized` = grant Automation in System Settings.
    /// `no_known_player` = nothing actionable (browser tab etc.).
    /// `other` = generic failure, detail has the codes.
    pub reason: &'static str,
    pub detail: String,
}

#[cfg(target_os = "macos")]
pub fn last_pause_diagnostic() -> Option<PauseDiagnostic> {
    mac::last_pause_diagnostic().map(to_ui)
}

#[cfg(not(target_os = "macos"))]
pub fn last_pause_diagnostic() -> Option<PauseDiagnostic> {
    None
}

/// Re-probe Automation TCC and return the resulting diagnostic. Mac-
/// only side-effect-free check — see `mac::probe_authorization`.
/// Other platforms have no per-app TCC layer that can silently deny,
/// so they always return `None`.
#[cfg(target_os = "macos")]
pub fn probe_authorization() -> Option<PauseDiagnostic> {
    mac::probe_authorization().map(to_ui)
}

#[cfg(not(target_os = "macos"))]
pub fn probe_authorization() -> Option<PauseDiagnostic> {
    None
}

#[cfg(target_os = "macos")]
fn to_ui(d: mac::PauseDiagnostic) -> PauseDiagnostic {
    PauseDiagnostic {
        reason: match d.reason {
            mac::PauseFailureReason::NotAuthorized => "not_authorized",
            mac::PauseFailureReason::NoKnownPlayer => "no_known_player",
            mac::PauseFailureReason::Other => "other",
        },
        detail: d.detail,
    }
}

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::WindowsMediaController as PlatformMediaController;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub struct PlatformMediaController;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
impl PlatformMediaController {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
impl MediaController for PlatformMediaController {
    fn pause_now(&self) -> bool {
        false
    }
    fn resume_now(&self) {}
}

