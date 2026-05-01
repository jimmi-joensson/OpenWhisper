//! Windows MediaController — stub. TASK-61.4 fills in SMTC
//! (GlobalSystemMediaTransportControlsSessionManager) for
//! media-session pause/play and IAudioEndpointVolume for the
//! endpoint-volume fade + mute fallback.

use super::MediaController;

pub struct WindowsMediaController;

impl WindowsMediaController {
    pub fn new() -> Self {
        Self
    }
}

impl MediaController for WindowsMediaController {
    fn pause_now(&self) -> bool {
        false
    }
    fn resume_now(&self) {}
}
