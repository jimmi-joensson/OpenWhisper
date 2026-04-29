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

use tauri::{AppHandle, Emitter};

use crate::settings::{self, BehaviorSettings};

static SHOW_IN_FULLSCREEN: AtomicBool = AtomicBool::new(false);

pub fn show_in_fullscreen() -> bool {
    SHOW_IN_FULLSCREEN.load(Ordering::Relaxed)
}

pub fn set_show_in_fullscreen_cache(value: bool) {
    SHOW_IN_FULLSCREEN.store(value, Ordering::Relaxed);
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
