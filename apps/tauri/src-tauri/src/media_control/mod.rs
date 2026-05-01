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

