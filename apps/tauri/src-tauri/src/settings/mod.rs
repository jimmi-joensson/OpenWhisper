//! Persistent user settings — currently the global toggle + cancel hotkey
//! descriptors.
//!
//! Stored as JSON at `<app_config_dir>/settings.json`. Hand-rolled (no
//! tauri-plugin-store) — small surface, single read at boot.
//!
//! Two-slot model: `toggle` (start/stop dictation) and `cancel` (discard
//! audio while recording). Both rebindable; defaults Right ⌘ / Esc on
//! mac, Ctrl+Space / Esc on Windows.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum HotkeyKind {
    /// Single modifier key, fired on release if no other key was pressed
    /// while it was held. Mac-only today.
    ModifierTap,
    /// Modifier(s) + non-modifier key, fired on the non-modifier KeyDown.
    Chord,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HotkeyConfig {
    pub kind: HotkeyKind,
    pub code: String,
    #[serde(default)]
    pub mods: Vec<String>,
}

impl HotkeyConfig {
    // ModifierTap is only used for the Mac default (RightCommand). The
    // Windows hotkey backend doesn't accept it, so this constructor is
    // dead on the Windows build — silence the warning rather than
    // sprinkle cfg gates at every call site.
    #[allow(dead_code)]
    pub fn modifier_tap(code: &str) -> Self {
        Self {
            kind: HotkeyKind::ModifierTap,
            code: code.to_string(),
            mods: Vec::new(),
        }
    }

    pub fn chord(code: &str, mods: &[&str]) -> Self {
        Self {
            kind: HotkeyKind::Chord,
            code: code.to_string(),
            mods: mods.iter().map(|s| s.to_string()).collect(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HotkeySettings {
    pub toggle: HotkeyConfig,
    pub cancel: HotkeyConfig,
}

/// Slot identifier — used by `settings_set_hotkey` and the capture flow
/// to disambiguate which binding the user is editing.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum HotkeyTarget {
    Toggle,
    Cancel,
}

impl HotkeyTarget {
    pub fn as_str(self) -> &'static str {
        match self {
            HotkeyTarget::Toggle => "toggle",
            HotkeyTarget::Cancel => "cancel",
        }
    }
}

pub fn default_toggle_hotkey() -> HotkeyConfig {
    #[cfg(target_os = "macos")]
    {
        HotkeyConfig::modifier_tap("RightCommand")
    }
    #[cfg(not(target_os = "macos"))]
    {
        HotkeyConfig::chord("Space", &["Ctrl"])
    }
}

pub fn default_cancel_hotkey() -> HotkeyConfig {
    HotkeyConfig::chord("Escape", &[])
}

pub fn default_settings() -> HotkeySettings {
    HotkeySettings {
        toggle: default_toggle_hotkey(),
        cancel: default_cancel_hotkey(),
    }
}

/// On-disk schema. `hotkey` is the legacy single-slot field — kept for
/// migration on first load after upgrading. `hotkeys` is the current
/// shape; we always write that.
#[derive(Serialize, Deserialize, Default)]
struct SettingsFile {
    #[serde(default)]
    hotkey: Option<HotkeyConfig>,
    #[serde(default)]
    hotkeys: Option<HotkeySettings>,
}

static CURRENT: Mutex<Option<HotkeySettings>> = Mutex::new(None);

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("app_config_dir: {e}"))?;
    Ok(dir.join("settings.json"))
}

fn read_file(app: &AppHandle) -> SettingsFile {
    let Ok(path) = settings_path(app) else {
        return SettingsFile::default();
    };
    let Ok(bytes) = fs::read(&path) else {
        return SettingsFile::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

fn write_file(app: &AppHandle, file: &SettingsFile) -> Result<(), String> {
    let path = settings_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create config dir: {e}"))?;
    }
    let bytes = serde_json::to_vec_pretty(file).map_err(|e| format!("serialize: {e}"))?;
    fs::write(&path, bytes).map_err(|e| format!("write settings.json: {e}"))?;
    Ok(())
}

fn merge_loaded(file: SettingsFile) -> HotkeySettings {
    if let Some(h) = file.hotkeys {
        return h;
    }
    HotkeySettings {
        toggle: file.hotkey.unwrap_or_else(default_toggle_hotkey),
        cancel: default_cancel_hotkey(),
    }
}

/// Load saved hotkeys (with migration from the legacy single-slot field)
/// or fall back to platform defaults. Caches the result for backends.
pub fn load_settings(app: &AppHandle) -> HotkeySettings {
    if let Some(s) = CURRENT.lock().ok().and_then(|g| g.clone()) {
        return s;
    }
    let file = read_file(app);
    let settings = merge_loaded(file);
    if let Ok(mut g) = CURRENT.lock() {
        *g = Some(settings.clone());
    }
    settings
}

pub fn current_settings() -> Option<HotkeySettings> {
    CURRENT.lock().ok().and_then(|g| g.clone())
}

fn save_settings(app: &AppHandle, settings: HotkeySettings) -> Result<(), String> {
    let file = SettingsFile {
        hotkey: None,
        hotkeys: Some(settings.clone()),
    };
    write_file(app, &file)?;
    if let Ok(mut g) = CURRENT.lock() {
        *g = Some(settings);
    }
    Ok(())
}

fn update_slot(
    app: &AppHandle,
    target: HotkeyTarget,
    config: HotkeyConfig,
) -> Result<(), String> {
    let mut current = current_settings().unwrap_or_else(default_settings);
    match target {
        HotkeyTarget::Toggle => current.toggle = config,
        HotkeyTarget::Cancel => current.cancel = config,
    }
    save_settings(app, current)
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
    update_slot(&app, target, config)?;
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
    update_slot(&app, target, cfg.clone())?;
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
