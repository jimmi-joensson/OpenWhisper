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
//! API. The status surface emits an ok=true on Win so the UI banner stays
//! hidden.
//!
//! Status surface mirrors `hotkey::HotkeyStatus`: a `permissions_status`
//! Tauri event with the current mic state and a `permissions_status_current`
//! command for cold-mounted UI windows. UI shows a HealthBanner when the
//! mic is denied or restricted.

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[cfg(target_os = "macos")]
mod mac;

pub const PERMISSIONS_STATUS_EVENT: &str = "permissions_status";

/// Coarse-grained UI-facing mic state. Auth-pending (`NotDetermined`) is
/// folded into `ok=true` so the banner doesn't flash before the user has
/// even seen the system dialog.
#[derive(Serialize, Clone, Debug)]
pub struct PermissionsStatus {
    pub mic_ok: bool,
    pub mic_state: &'static str,
    pub error: String,
}

static LAST_STATUS: Mutex<Option<PermissionsStatus>> = Mutex::new(None);

pub(crate) fn emit_status(
    app: &AppHandle,
    mic_ok: bool,
    mic_state: &'static str,
    error: impl Into<String>,
) {
    let status = PermissionsStatus {
        mic_ok,
        mic_state,
        error: error.into(),
    };
    if let Ok(mut last) = LAST_STATUS.lock() {
        *last = Some(status.clone());
    }
    if let Err(e) = app.emit(PERMISSIONS_STATUS_EVENT, &status) {
        eprintln!("permissions_status emit failed: {e}");
    }
}

/// Fire the system mic prompt when warranted + emit current status.
/// Idempotent — safe to call every boot. Silent no-op on the prompt side
/// when mic is already authorized / denied / restricted, or when AX is
/// not yet trusted (matches Swift sequencing); status is still emitted
/// so the UI sees the current state.
pub fn request_microphone(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        mac::request_microphone(app);
    }
    #[cfg(not(target_os = "macos"))]
    {
        // Win: no programmatic prompt; the OS handles consent at first
        // device open. Surface ok so the UI banner stays hidden.
        emit_status(app, true, "authorized", "");
    }
}

/// Returns the last status emitted via `permissions_status`. UI calls this
/// on mount so it can render the right banner state without racing the
/// boot probe emit.
#[tauri::command]
pub fn permissions_status_current() -> Option<PermissionsStatus> {
    LAST_STATUS.lock().ok().and_then(|g| g.clone())
}
