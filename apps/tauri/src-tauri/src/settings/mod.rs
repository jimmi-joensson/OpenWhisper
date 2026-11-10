//! Tauri-shell facade over [`openwhisper_core::settings`].
//!
//! Schema, IO, and lock-free caches all live in core. This module
//! resolves the Tauri-specific config-dir path
//! (`app.path().app_config_dir() / "settings.json"`) and exposes the
//! `#[tauri::command]` wrappers the React side invokes.

use std::path::PathBuf;

use openwhisper_core::dictionary::{self as dictionary, DictionaryTerm};
use openwhisper_core::settings as core;
use tauri::{AppHandle, Emitter, Manager};

// Re-exports used by other shell modules (`crate::settings::X`) and
// by this module's Tauri commands. Schema types + a small set of
// accessors. Hot-path readers (fullscreen detector, dictation
// observer) reach `openwhisper_core::settings::X` directly.
pub use core::{
    AudioSettings, BehaviorSettings, HotkeyConfig, HotkeyKind, HotkeySettings, HotkeyTarget,
    PerformanceSettings, PillSettings, StatsSettings, current_pill_settings, current_settings,
    current_stats_settings, default_cancel_hotkey, default_settings,
    default_toggle_hotkey, follow_active_screen,
};

pub(crate) fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("app_config_dir: {e}"))?;
    Ok(dir.join("settings.json"))
}

/// Boot-time hydrate. Returns the cached or freshly-read hotkey
/// block; side-effects: populates the hotkey cache and
/// `follow_active_screen` mirror via core.
pub fn load_settings(app: &AppHandle) -> HotkeySettings {
    match settings_path(app) {
        Ok(p) => core::load_settings(&p),
        Err(_) => default_settings(),
    }
}

pub fn load_stats_settings(app: &AppHandle) -> StatsSettings {
    match settings_path(app) {
        Ok(p) => core::load_stats_settings(&p),
        Err(_) => StatsSettings::default(),
    }
}

pub fn load_audio_settings(app: &AppHandle) -> AudioSettings {
    match settings_path(app) {
        Ok(p) => core::load_audio_settings(&p),
        Err(_) => AudioSettings::default(),
    }
}

pub fn load_behavior_settings(app: &AppHandle) -> BehaviorSettings {
    match settings_path(app) {
        Ok(p) => core::load_behavior_settings(&p),
        Err(_) => BehaviorSettings::default(),
    }
}

pub fn load_performance_settings(app: &AppHandle) -> PerformanceSettings {
    match settings_path(app) {
        Ok(p) => core::load_performance_settings(&p),
        Err(_) => PerformanceSettings::default(),
    }
}

pub fn save_behavior_settings(
    app: &AppHandle,
    settings: BehaviorSettings,
) -> Result<(), String> {
    let path = settings_path(app)?;
    core::save_behavior_settings(&path, settings)
}

#[tauri::command]
pub fn settings_get_hotkeys(app: AppHandle) -> HotkeySettings {
    load_settings(&app)
}

#[tauri::command]
pub fn settings_set_hotkey(
    app: AppHandle,
    target: HotkeyTarget,
    config: HotkeyConfig,
) -> Result<(), String> {
    let path = settings_path(&app)?;
    core::update_hotkey_slot(&path, target, config)?;
    crate::hotkey::install(&app);
    Ok(())
}

#[tauri::command]
pub fn settings_reset_hotkey(
    app: AppHandle,
    target: HotkeyTarget,
) -> Result<HotkeyConfig, String> {
    let cfg = match target {
        HotkeyTarget::Toggle => default_toggle_hotkey(),
        HotkeyTarget::Cancel => default_cancel_hotkey(),
    };
    let path = settings_path(&app)?;
    core::update_hotkey_slot(&path, target, cfg.clone())?;
    crate::hotkey::install(&app);
    Ok(cfg)
}

#[tauri::command]
pub fn settings_capture_hotkey_start(target: HotkeyTarget) {
    crate::hotkey::set_capture_active(true, Some(target));
}

#[tauri::command]
pub fn settings_capture_hotkey_cancel() {
    crate::hotkey::set_capture_active(false, None);
}

#[tauri::command]
pub fn audio_set_device(app: AppHandle, id: Option<String>) -> Result<(), String> {
    let normalized = id.and_then(|s| {
        let trimmed = s.trim().to_string();
        if trimmed.is_empty() { None } else { Some(trimmed) }
    });
    let settings = AudioSettings { device_id: normalized.clone() };
    let path = settings_path(&app)?;
    core::save_audio_settings(&path, settings)?;
    openwhisper_core::audio::audio_set_selected_device_id(normalized);
    Ok(())
}

#[tauri::command]
pub fn settings_get_pill(_app: AppHandle) -> PillSettings {
    current_pill_settings()
}

#[tauri::command]
pub fn settings_set_pill_follow(app: AppHandle, follow: bool) -> Result<(), String> {
    let path = settings_path(&app)?;
    core::save_pill_settings(&path, PillSettings { follow_active_screen: follow })
}

#[tauri::command]
pub fn settings_get_stats(app: AppHandle) -> StatsSettings {
    load_stats_settings(&app)
}

/// Read the persisted Performance settings (currently just
/// `keep_models_warm`). Returns the default (`false`) on first run
/// before any save has happened.
#[tauri::command]
pub fn settings_get_performance(app: AppHandle) -> PerformanceSettings {
    load_performance_settings(&app)
}

/// Persist the keep-models-warm flag and push it into every
/// registered `ModelHandle` in the same call. Order: write JSON →
/// flip lock-free atomic (handled inside core's
/// `save_performance_settings`) → call `apply_keep_warm` so live
/// handles reconfigure without an app restart. Per the
/// model-lifecycle spec (`backlog/docs/specs/2026-05-01-model-
/// lifecycle-telemetry.md`) the setter is the single source of
/// truth for in-process effect; reads come from
/// `openwhisper_core::settings::keep_models_warm()`.
#[tauri::command]
pub fn settings_set_keep_models_warm(app: AppHandle, value: bool) -> Result<(), String> {
    let path = settings_path(&app)?;
    core::save_performance_settings(&path, PerformanceSettings { keep_models_warm: value })?;
    openwhisper_core::model_lifecycle::apply_keep_warm(value);
    Ok(())
}

/// Persist a new typing-speed calibration. Out-of-range writes are
/// silently clamped to [`USER_WPM_MIN`, `USER_WPM_MAX`] inside the
/// core save path. Emits `settings_stats_changed` so the React
/// `useUserWpm` hook can refresh without a manual round-trip.
#[tauri::command]
pub fn settings_set_user_wpm(app: AppHandle, wpm: u32) -> Result<u32, String> {
    let path = settings_path(&app)?;
    core::save_stats_settings(&path, StatsSettings { user_wpm: wpm })?;
    let stored = current_stats_settings().unwrap_or_default().user_wpm;
    let _ = app.emit("settings_stats_changed", stored);
    Ok(stored)
}

// --- Dictionary ----------------------------------------------------
//
// `dictionary.json` lives next to `settings.json`. Core owns parsing,
// matching and the mtime reload; the shell only resolves the path,
// points core at it at boot, and exposes read/save/reveal commands.

fn dictionary_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("app_config_dir: {e}"))?;
    Ok(dir.join(dictionary::FILE_NAME))
}

/// Boot-time: point core's transcript post-pass at the user's file.
pub fn init_dictionary(app: &AppHandle) {
    if let Ok(p) = dictionary_path(app) {
        dictionary::set_path(&p);
    }
}

#[derive(serde::Serialize)]
pub struct DictionaryState {
    pub path: String,
    pub terms: Vec<DictionaryTerm>,
}

#[tauri::command]
pub fn dictionary_get(app: AppHandle) -> Result<DictionaryState, String> {
    let path = dictionary_path(&app)?;
    let dict = dictionary::load(&path)?;
    Ok(DictionaryState {
        path: path.display().to_string(),
        terms: dict.terms,
    })
}

/// Replace the whole term list. Core trims, merges duplicates and
/// sorts; the stored result is returned so the UI shows what the file
/// now holds.
#[tauri::command]
pub fn dictionary_save(
    app: AppHandle,
    terms: Vec<DictionaryTerm>,
) -> Result<DictionaryState, String> {
    let path = dictionary_path(&app)?;
    let saved = dictionary::save(&path, &dictionary::Dictionary { terms })?;
    Ok(DictionaryState {
        path: path.display().to_string(),
        terms: saved.terms,
    })
}

/// Show `dictionary.json` in Finder / Explorer, creating an empty one
/// first so there is something to select.
#[tauri::command]
pub fn dictionary_reveal(app: AppHandle) -> Result<(), String> {
    let path = dictionary_path(&app)?;
    if !path.exists() {
        dictionary::save(&path, &dictionary::Dictionary::default())?;
    }

    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = std::process::Command::new("open");
        c.arg("-R").arg(&path);
        c
    };
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = std::process::Command::new("explorer");
        c.arg(format!("/select,{}", path.display()));
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(path.parent().unwrap_or(&path));
        c
    };

    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("reveal {}: {e}", path.display()))
}
