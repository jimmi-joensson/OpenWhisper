//! Tauri commands for the `behavior` settings block + the NSPanel
//! collection-behavior platform glue.
//!
//! Schema, persistence, and lock-free caches all live in
//! [`openwhisper_core::settings`]. This module only owns:
//!   - the `#[tauri::command]` wrappers React invokes,
//!   - [`apply_collection_behavior`], which mirrors the
//!     `show_in_fullscreen` setting onto the pill panel's and the
//!     answer panel's NSCollectionBehavior on macOS /
//!     `set_visible_on_all_workspaces` on Windows.

use openwhisper_core::settings::{
    self, BehaviorSettings, BT_RESUME_DELAY_MS_MAX,
};
use tauri::{AppHandle, Emitter};
#[cfg(not(target_os = "macos"))]
use tauri::Manager;

fn current_or_default() -> BehaviorSettings {
    settings::current_behavior_settings().unwrap_or_default()
}

/// The windows that follow the "show in fullscreen" rule: the pill
/// and the answer window that sits above it.
const OVERLAY_WINDOWS: [&str; 2] = ["pill", crate::ask::ANSWER_LABEL];

/// Mirror `show_in_fullscreen` onto the overlay panels'
/// collection-behavior so they can render over other apps' fullscreen
/// Spaces on macOS.
///
/// On macOS the pill and the answer window are converted to NSPanels
/// at boot (see `lib.rs::setup`) and we drive their
/// collection-behavior through the `tauri-nspanel` API rather than
/// the underlying NSWindow's `set_visible_on_all_workspaces`: plain
/// NSWindow with the same bits is unreliable on Sonoma+ when the
/// fullscreen Space owner is another app (Apple Developer Forums
/// #26677). The panel's `nonactivating_panel` style +
/// `full_screen_auxiliary` + `can_join_all_spaces` is the canonical
/// recipe shipped by Cap, Screenpipe, Hyprnote, Wispr Flow.
///
/// On non-macOS targets we fall back to the Tauri call, which is a
/// no-op on platforms that don't support the workspace concept (the
/// pill follows the active virtual desktop on Windows already).
pub fn apply_collection_behavior(app: &AppHandle, show: bool) {
    #[cfg(target_os = "macos")]
    {
        use tauri_nspanel::{CollectionBehavior, ManagerExt};
        for label in OVERLAY_WINDOWS {
            let Ok(panel) = app.get_webview_panel(label) else {
                continue;
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
    }
    #[cfg(not(target_os = "macos"))]
    {
        for label in OVERLAY_WINDOWS {
            if let Some(window) = app.get_webview_window(label) {
                let _ = window.set_visible_on_all_workspaces(show);
            }
        }
    }
}

#[tauri::command]
pub fn behavior_get_show_in_fullscreen() -> bool {
    settings::show_in_fullscreen()
}

#[tauri::command]
pub fn behavior_set_show_in_fullscreen(
    app: AppHandle,
    enabled: bool,
) -> Result<(), String> {
    let mut next = current_or_default();
    next.show_in_fullscreen = enabled;
    crate::settings::save_behavior_settings(&app, next)?;
    settings::set_show_in_fullscreen_cache(enabled);
    app.emit("behavior_show_in_fullscreen_changed", enabled)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn behavior_get_pause_audio_during_dictation() -> bool {
    settings::pause_audio_during_dictation()
}

#[tauri::command]
pub fn behavior_set_pause_audio_during_dictation(
    app: AppHandle,
    enabled: bool,
) -> Result<(), String> {
    let mut next = current_or_default();
    next.pause_audio_during_dictation = enabled;
    crate::settings::save_behavior_settings(&app, next)?;
    settings::set_pause_audio_cache(enabled);
    app.emit("behavior_pause_audio_changed", enabled)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn behavior_get_bt_resume_delay_ms() -> u64 {
    settings::bt_resume_delay_ms()
}

/// Clamp + persist + cache + emit. UI should already validate
/// in-range, but a Rust-side clamp is the source of truth so a
/// malformed `settings.json` doesn't ship a 0 ms or 60-second delay
/// to the MediaController.
#[tauri::command]
pub fn behavior_set_bt_resume_delay_ms(
    app: AppHandle,
    delay_ms: u64,
) -> Result<(), String> {
    // Hard bounds: 0 disables the wait entirely (BT users on faster
    // radios who'd rather get instant resume + accept the mono
    // chance), BT_RESUME_DELAY_MS_MAX (10000 ms) caps the worst-case
    // stuck-HFP wait.
    let clamped = delay_ms.min(BT_RESUME_DELAY_MS_MAX);
    let mut next = current_or_default();
    next.bt_resume_delay_ms = clamped;
    crate::settings::save_behavior_settings(&app, next)?;
    settings::set_bt_resume_delay_ms_cache(clamped);
    app.emit("behavior_bt_resume_delay_changed", clamped)
        .map_err(|e| e.to_string())?;
    Ok(())
}
