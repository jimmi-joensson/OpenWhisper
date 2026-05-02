//! Behavior settings — `show_in_fullscreen` (override fullscreen
//! suppression) and `pause_audio_during_dictation` (auto-pause other
//! apps' playback while recording).
//!
//! Hot-path readers (fullscreen detector callback, dictation phase
//! observer) read the AtomicBool caches on every tick without
//! round-tripping through the settings file or the WebView. The
//! setter commands persist the value, update the cache, and emit a
//! `behavior_*_changed` event so React surfaces refresh and lib.rs
//! listeners can reconcile.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use tauri::{AppHandle, Emitter};
#[cfg(not(target_os = "macos"))]
use tauri::Manager;

use crate::settings::{self, default_bt_resume_delay_ms, BehaviorSettings};

static SHOW_IN_FULLSCREEN: AtomicBool = AtomicBool::new(false);
static PAUSE_AUDIO: AtomicBool = AtomicBool::new(true);
/// Cached `BehaviorSettings::bt_resume_delay_ms`. Read by the platform
/// MediaController on every resume, so we keep this lock-free. Default
/// matches the schema default (5000) so any read before
/// `set_bt_resume_delay_ms_cache` runs at boot still gets a sane
/// value rather than 0 (which would defeat the purpose of the wait).
static BT_RESUME_DELAY_MS: AtomicU64 = AtomicU64::new(5000);

pub fn show_in_fullscreen() -> bool {
    SHOW_IN_FULLSCREEN.load(Ordering::Relaxed)
}

pub fn set_show_in_fullscreen_cache(value: bool) {
    SHOW_IN_FULLSCREEN.store(value, Ordering::Relaxed);
}

pub fn pause_audio_during_dictation() -> bool {
    PAUSE_AUDIO.load(Ordering::Relaxed)
}

pub fn set_pause_audio_cache(value: bool) {
    PAUSE_AUDIO.store(value, Ordering::Relaxed);
}

pub fn bt_resume_delay_ms() -> u64 {
    BT_RESUME_DELAY_MS.load(Ordering::Relaxed)
}

pub fn set_bt_resume_delay_ms_cache(value: u64) {
    BT_RESUME_DELAY_MS.store(value, Ordering::Relaxed);
}

fn current_or_default() -> BehaviorSettings {
    settings::current_behavior_settings().unwrap_or_default()
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
    let mut next = current_or_default();
    next.show_in_fullscreen = enabled;
    settings::save_behavior_settings(&app, next)?;
    set_show_in_fullscreen_cache(enabled);
    app.emit("behavior_show_in_fullscreen_changed", enabled)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn behavior_get_pause_audio_during_dictation() -> bool {
    pause_audio_during_dictation()
}

#[tauri::command]
pub fn behavior_set_pause_audio_during_dictation(
    app: AppHandle,
    enabled: bool,
) -> Result<(), String> {
    let mut next = current_or_default();
    next.pause_audio_during_dictation = enabled;
    settings::save_behavior_settings(&app, next)?;
    set_pause_audio_cache(enabled);
    app.emit("behavior_pause_audio_changed", enabled)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn behavior_get_bt_resume_delay_ms() -> u64 {
    bt_resume_delay_ms()
}

/// Clamp + persist + cache + emit. UI should already validate in-range,
/// but a Rust-side clamp is the source of truth so a malformed
/// settings.json doesn't ship a 0 ms or 60-second delay to the
/// MediaController.
#[tauri::command]
pub fn behavior_set_bt_resume_delay_ms(
    app: AppHandle,
    delay_ms: u64,
) -> Result<(), String> {
    // Hard bounds: 0 disables the wait entirely (BT users on faster
    // radios who'd rather get instant resume + accept the mono
    // chance), 10000 caps the worst-case stuck-HFP wait. If a user
    // really needs a longer delay, it's a config-file edit and we'll
    // honor it on next save round-trip via clamp here.
    let clamped = delay_ms.min(10_000);
    let mut next = current_or_default();
    next.bt_resume_delay_ms = clamped;
    settings::save_behavior_settings(&app, next)?;
    set_bt_resume_delay_ms_cache(clamped);
    app.emit("behavior_bt_resume_delay_changed", clamped)
        .map_err(|e| e.to_string())?;
    Ok(())
}

// Keep the schema-default helper visible to `lib.rs::setup` so the
// boot-time hydrate path uses the same constant the schema does — one
// source of truth.
#[allow(dead_code)]
pub fn schema_default_bt_resume_delay_ms() -> u64 {
    default_bt_resume_delay_ms()
}
