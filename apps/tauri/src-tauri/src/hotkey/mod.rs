//! Global hotkey + Escape-to-cancel.
//!
//! Per-platform because activation gestures differ deliberately:
//! - **Mac**: Right Command tap-not-hold (Right Cmd held as a chord modifier
//!   does *not* fire), via a `core-graphics` CGEventTap. Plugin can't do it.
//! - **Windows**: `Ctrl+Space` chord via `tauri-plugin-global-shortcut`.
//!
//! Escape-to-cancel rides on the same OS-level keyboard surface in both
//! cases. Core's `dictation_request_cancel` is phase-gated, so the hook
//! never has to know whether we're recording — fire on every Escape, core
//! ignores when irrelevant.
//!
//! Status surface: `hotkey_status` Tauri event (see [`HotkeyStatus`]) and
//! the `hotkey_retry` command. UI listens to the event and shows the
//! HealthBanner with a Retry button when `ok = false`.

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[cfg(target_os = "macos")]
mod mac;
#[cfg(target_os = "windows")]
mod windows;

/// Pushed to the front-end on registration success / failure / watchdog
/// re-enable. `error` is empty when `ok = true`. UI shows a HealthBanner
/// when `ok = false`.
#[derive(Serialize, Clone, Debug)]
pub struct HotkeyStatus {
    pub ok: bool,
    pub error: String,
}

pub const HOTKEY_STATUS_EVENT: &str = "hotkey_status";

/// Last status — kept so newly mounted UI windows can pull the current
/// state via a future `hotkey_status_current` command. Not used yet.
static LAST_STATUS: Mutex<Option<HotkeyStatus>> = Mutex::new(None);

pub(crate) fn emit_status(app: &AppHandle, ok: bool, error: impl Into<String>) {
    let status = HotkeyStatus { ok, error: error.into() };
    if let Ok(mut last) = LAST_STATUS.lock() {
        *last = Some(status.clone());
    }
    if let Err(e) = app.emit(HOTKEY_STATUS_EVENT, &status) {
        eprintln!("hotkey_status emit failed: {e}");
    }
}

/// Install platform-specific hotkey + escape hook. Idempotent — calling
/// twice tears down and reinstalls. Used both at boot and from
/// `hotkey_retry`.
pub fn install(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        match mac::install(app) {
            Ok(()) => emit_status(app, true, ""),
            Err(e) => emit_status(app, false, e),
        }
    }
    #[cfg(target_os = "windows")]
    {
        match windows::install(app) {
            Ok(()) => emit_status(app, true, ""),
            Err(e) => emit_status(app, false, e),
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        emit_status(app, false, "hotkey: platform not supported in MVP");
    }
}

#[tauri::command]
pub fn hotkey_retry(app: AppHandle) {
    install(&app);
}
