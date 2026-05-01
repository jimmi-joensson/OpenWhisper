//! Global hotkey + cancel-while-recording.
//!
//! Two configurable slots — toggle (start/stop dictation) and cancel
//! (discard the current recording). Defaults: Right ⌘ / Esc on mac,
//! Ctrl+Space / Esc on Windows. Both rebindable via Settings → Shortcuts.
//!
//! Per-platform because activation gestures differ:
//! - **Mac**: `CGEventTap` — toggle supports both modifier-tap (default
//!   Right Cmd: tap-not-hold) and chord; cancel supports the same.
//! - **Windows**: `WH_KEYBOARD_LL` hook — chord-only for both slots.
//!
//! Status surface: `hotkey_status` Tauri event (see [`HotkeyStatus`]) and
//! the `hotkey_retry` command. UI shows a HealthBanner when `ok = false`.
//!
//! Capture surface: Settings flips [`set_capture_active`] true while the
//! user clicks "press keys…". The next eligible event is delivered via
//! the `hotkey_captured` Tauri event (payload `{ target, config }`) and
//! capture mode auto-disables.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::settings::{HotkeyConfig, HotkeyTarget};

#[cfg(target_os = "macos")]
mod mac;
#[cfg(target_os = "windows")]
mod windows;

#[derive(Serialize, Clone, Debug)]
pub struct HotkeyStatus {
    pub ok: bool,
    pub error: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct HotkeyCapturedPayload {
    pub target: &'static str,
    pub config: HotkeyConfig,
}

pub const HOTKEY_STATUS_EVENT: &str = "hotkey_status";
pub const HOTKEY_CAPTURED_EVENT: &str = "hotkey_captured";

static LAST_STATUS: Mutex<Option<HotkeyStatus>> = Mutex::new(None);

/// Stored at first install — backends emit capture results from worker
/// threads so we cache the handle here rather than threading it through
/// the per-platform tap state.
static APP_HANDLE: OnceLock<Mutex<Option<AppHandle>>> = OnceLock::new();

static CAPTURE_ACTIVE: AtomicBool = AtomicBool::new(false);
static CAPTURE_TARGET: Mutex<Option<HotkeyTarget>> = Mutex::new(None);

fn handle_slot() -> &'static Mutex<Option<AppHandle>> {
    APP_HANDLE.get_or_init(|| Mutex::new(None))
}

fn store_handle(app: &AppHandle) {
    if let Ok(mut g) = handle_slot().lock() {
        *g = Some(app.clone());
    }
}

fn cached_handle() -> Option<AppHandle> {
    handle_slot().lock().ok().and_then(|g| g.clone())
}

pub(crate) fn emit_status(app: &AppHandle, ok: bool, error: impl Into<String>) {
    let status = HotkeyStatus { ok, error: error.into() };
    if let Ok(mut last) = LAST_STATUS.lock() {
        *last = Some(status.clone());
    }
    if let Err(e) = app.emit(HOTKEY_STATUS_EVENT, &status) {
        eprintln!("hotkey_status emit failed: {e}");
    }
}

/// Toggle capture-mode on/off. While true, the next eligible hotkey event
/// is captured and delivered via `hotkey_captured` rather than firing the
/// real toggle/cancel. Backend-agnostic — both `mac.rs` and `windows.rs`
/// poll this from their callbacks.
pub fn set_capture_active(active: bool, target: Option<HotkeyTarget>) {
    if active {
        if let (true, Some(t)) = (active, target) {
            if let Ok(mut g) = CAPTURE_TARGET.lock() {
                *g = Some(t);
            }
        }
    } else if let Ok(mut g) = CAPTURE_TARGET.lock() {
        *g = None;
    }
    CAPTURE_ACTIVE.store(active, Ordering::Relaxed);
}

pub fn is_capture_active() -> bool {
    CAPTURE_ACTIVE.load(Ordering::Relaxed)
}

fn current_capture_target() -> Option<HotkeyTarget> {
    CAPTURE_TARGET.lock().ok().and_then(|g| *g)
}

/// Emit a captured descriptor to the front-end and exit capture mode.
/// Called by the per-platform backends from the hook/tap thread.
pub(crate) fn deliver_capture(config: HotkeyConfig) {
    let target = current_capture_target().unwrap_or(HotkeyTarget::Toggle);
    CAPTURE_ACTIVE.store(false, Ordering::Relaxed);
    if let Ok(mut g) = CAPTURE_TARGET.lock() {
        *g = None;
    }
    let Some(app) = cached_handle() else {
        return;
    };
    let payload = HotkeyCapturedPayload {
        target: target.as_str(),
        config,
    };
    if let Err(e) = app.emit(HOTKEY_CAPTURED_EVENT, &payload) {
        eprintln!("hotkey_captured emit failed: {e}");
    }
}

/// Toggle the global hotkey on/off. Used by the fullscreen-aware path.
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

/// Install platform-specific hotkey + cancel hook. Idempotent — calling
/// twice tears down and reinstalls. Used at boot, from `hotkey_retry`,
/// and after any `settings_set_hotkey` save.
pub fn install(app: &AppHandle) {
    store_handle(app);
    let settings =
        crate::settings::current_settings().unwrap_or_else(crate::settings::default_settings);
    #[cfg(target_os = "macos")]
    {
        match mac::install(app, &settings) {
            Ok(()) => emit_status(app, true, ""),
            Err(e) => emit_status(app, false, e),
        }
    }
    #[cfg(target_os = "windows")]
    {
        match windows::install(app, &settings) {
            Ok(()) => emit_status(app, true, ""),
            Err(e) => emit_status(app, false, e),
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = settings;
        emit_status(app, false, "hotkey: platform not supported in MVP");
    }
}

#[tauri::command]
pub fn hotkey_retry(app: AppHandle) {
    #[cfg(target_os = "macos")]
    {
        // Tauri 2's app.restart() re-execs current_exe() directly. On Mac
        // that skips LaunchServices/launchctl registration, so the process
        // exits but the new instance silently fails to start — especially
        // in dev builds where ad-hoc signing + TCC handoff is fragile.
        // Canonical Mac relaunch is `open -n -a <bundle>`, which goes
        // through LaunchServices and registers with launchctl cleanly.
        // Walk current_exe ancestors to find the enclosing .app bundle.
        if let Ok(exe) = std::env::current_exe() {
            if let Some(bundle) = exe.ancestors().find(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .map(|s| s == "app")
                    .unwrap_or(false)
            }) {
                let _ = std::process::Command::new("open")
                    .args(["-n", "-a"])
                    .arg(bundle)
                    .spawn();
                app.exit(0);
                return;
            }
        }
        // Fallback for the bare-binary case (running target/debug/openwhisper-tauri
        // without a .app wrapper). Tauri's restart() is correct there.
        app.restart();
    }
    #[cfg(target_os = "windows")]
    {
        install(&app);
    }
}

#[tauri::command]
pub fn hotkey_status_current() -> Option<HotkeyStatus> {
    LAST_STATUS.lock().ok().and_then(|g| g.clone())
}
