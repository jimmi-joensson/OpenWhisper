//! Behavior settings — currently the single `show_in_fullscreen` toggle
//! that lets the user override OpenWhisper's automatic deactivation when
//! another app is fullscreen on the focused screen.
//!
//! The fullscreen detector callback (registered in `lib.rs`) reads the
//! AtomicBool cache on every transition without round-tripping through
//! the settings file or the WebView. The setter command persists the
//! value, updates the cache, and emits `behavior_show_in_fullscreen_changed`
//! so React surfaces refresh and the lib.rs listener can reconcile pill
//! visibility / hotkey state.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Emitter, Manager};

use crate::settings::{self, BehaviorSettings};

static SHOW_IN_FULLSCREEN: AtomicBool = AtomicBool::new(false);

pub fn show_in_fullscreen() -> bool {
    SHOW_IN_FULLSCREEN.load(Ordering::Relaxed)
}

pub fn set_show_in_fullscreen_cache(value: bool) {
    SHOW_IN_FULLSCREEN.store(value, Ordering::Relaxed);
}

/// Mirror `show_in_fullscreen` onto the pill window's `visibleOnAllWorkspaces`
/// collection-behavior bit. On macOS this is the AppKit
/// `canJoinAllSpaces`/`fullScreenAuxiliary` combo that lets the pill draw
/// over fullscreen Spaces — without it, a normal-level window stays trapped
/// in its origin Space and is invisible while a fullscreen app owns the
/// screen. Tauri documents the call as a no-op on platforms that don't
/// support it (Windows virtual desktops are a different model and the
/// pill follows the active desktop already), so this is safe to call
/// unconditionally.
pub fn apply_collection_behavior(app: &AppHandle, show: bool) {
    if let Some(pill) = app.get_webview_window("pill") {
        let _ = pill.set_visible_on_all_workspaces(show);
    }
}

#[tauri::command]
pub fn behavior_get_show_in_fullscreen() -> bool {
    show_in_fullscreen()
}

#[tauri::command]
pub fn behavior_set_show_in_fullscreen(
    app: AppHandle,
    enabled: bool,
) -> Result<(), String> {
    settings::save_behavior_settings(
        &app,
        BehaviorSettings { show_in_fullscreen: enabled },
    )?;
    set_show_in_fullscreen_cache(enabled);
    app.emit("behavior_show_in_fullscreen_changed", enabled)
        .map_err(|e| e.to_string())?;
    Ok(())
}
