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
#[cfg(not(target_os = "macos"))]
use tauri::Manager;

use crate::settings::{self, BehaviorSettings};

static SHOW_IN_FULLSCREEN: AtomicBool = AtomicBool::new(false);

pub fn show_in_fullscreen() -> bool {
    SHOW_IN_FULLSCREEN.load(Ordering::Relaxed)
}

pub fn set_show_in_fullscreen_cache(value: bool) {
    SHOW_IN_FULLSCREEN.store(value, Ordering::Relaxed);
}

/// Mirror `show_in_fullscreen` onto the pill panel's collection-behavior
/// so it can render over other apps' fullscreen Spaces on macOS.
///
/// On macOS the pill is converted to an NSPanel at boot (see
/// `lib.rs::setup`) and we drive its collection-behavior through the
/// `tauri-nspanel` API rather than the underlying NSWindow's
/// `set_visible_on_all_workspaces`: plain NSWindow with the same bits
/// is unreliable on Sonoma+ when the fullscreen Space owner is
/// another app (Apple Developer Forums #26677). The panel's
/// `nonactivating_panel` style + `full_screen_auxiliary` +
/// `can_join_all_spaces` is the canonical recipe shipped by Cap,
/// Screenpipe, Hyprnote, Wispr Flow.
///
/// On non-macOS targets we fall back to the Tauri call, which is a
/// no-op on platforms that don't support the workspace concept (the
/// pill follows the active virtual desktop on Windows already).
pub fn apply_collection_behavior(app: &AppHandle, show: bool) {
    #[cfg(target_os = "macos")]
    {
        use tauri_nspanel::{CollectionBehavior, ManagerExt};
        let Ok(panel) = app.get_webview_panel("pill") else {
            return;
        };
        let cb = if show {
            CollectionBehavior::new()
                .can_join_all_spaces()
                .full_screen_auxiliary()
        } else {
            CollectionBehavior::new()
        };
        panel.set_collection_behavior(cb.into());
    }
    #[cfg(not(target_os = "macos"))]
    {
        if let Some(pill) = app.get_webview_window("pill") {
            let _ = pill.set_visible_on_all_workspaces(show);
        }
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
