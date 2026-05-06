//! Platform `MediaController` impl. Trait + diagnostic types live in
//! [`openwhisper_core::media_gate`]; this module exposes the platform
//! impl as `PlatformMediaController` and provides the Linux fallback
//! that keeps non-Mac/Win builds compiling.

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
impl openwhisper_core::media_gate::MediaController for PlatformMediaController {
    fn pause_now(&self) -> bool {
        false
    }
    fn resume_now(&self) {}
}
