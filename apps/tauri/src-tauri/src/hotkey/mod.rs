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

// On macOS the kernel-side TCC cache for Accessibility doesn't refresh until
// the process relaunches. Granting in System Settings while we're running
// flips `AXIsProcessTrusted` to true but `CGEventTapCreate` keeps returning
// nil — so a manual Retry can't recover. We detect this case in
// `hotkey_retry` and call `app.restart()`.
#[cfg(target_os = "macos")]
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
}

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

/// Toggle the global hotkey on/off. Used by the fullscreen-aware path:
/// when a fullscreen app takes the foreground we tear down the system
/// surface entirely (Mac CGEventTap, Win Ctrl+Space chord + Escape hook)
/// so the fullscreen app receives those keystrokes normally and
/// OpenWhisper doesn't activate. Re-armed on fullscreen exit.
///
/// Status events are NOT emitted for fullscreen-driven flips — the user
/// hasn't lost permission, they just opened a fullscreen app. The pill
/// hides at the same time so the absence of dictation is unambiguous.
pub fn set_active(app: &AppHandle, active: bool) {
    if active {
        install(app);
    } else {
        #[cfg(target_os = "macos")]
        mac::teardown();
        #[cfg(target_os = "windows")]
        windows::teardown(app);
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let _ = app;
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

    #[cfg(target_os = "macos")]
    {
        // If install still fails AND the user has now granted Accessibility
        // (TCC says trusted), the kernel-side cache is what's stale — only
        // a relaunch fixes it. Restart so the new grant lands.
        let needs_restart = {
            let still_failing = LAST_STATUS
                .lock()
                .ok()
                .and_then(|g| g.as_ref().map(|s| !s.ok))
                .unwrap_or(false);
            still_failing && unsafe { AXIsProcessTrusted() }
        };
        if needs_restart {
            app.restart();
        }
    }
}

/// Returns the last status emitted via `hotkey_status`. UI calls this on
/// mount so it can render the right banner state without racing the boot
/// install emit.
#[tauri::command]
pub fn hotkey_status_current() -> Option<HotkeyStatus> {
    LAST_STATUS.lock().ok().and_then(|g| g.clone())
}
