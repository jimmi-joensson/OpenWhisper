//! Transcript history commands (TASK-95). Thin wrappers: core owns
//! the settings block, the Markdown day files and the retention rule;
//! this module resolves paths and hands the calls through.
//!
//! The same functions are reachable headless through
//! `openwhisper history …`.

use std::path::PathBuf;

use openwhisper_core::history::{self, HistoryPage};
use openwhisper_core::settings::{self as core_settings, HistorySettings};
use tauri::{AppHandle, Manager};

use crate::settings::settings_path;

/// How many entries a list call returns when the webview does not say.
const DEFAULT_PAGE: u32 = 25;

/// What Home, the History screen and the Settings section load.
#[derive(serde::Serialize)]
pub struct HistoryState {
    pub settings: HistorySettings,
    /// The folder the day files are written to, shown to the user
    /// as-is.
    pub folder: String,
    /// The folder used when the user has not chosen one.
    pub default_folder: String,
}

/// `history/` in the app's config folder, next to `dictionary.json`.
fn default_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("app_config_dir: {e}"))?;
    Ok(dir.join(history::DEFAULT_FOLDER_NAME))
}

/// The folder in use: the user's choice, else the default.
fn history_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let settings = core_settings::current_history_settings();
    Ok(history::resolve_dir(&settings, &default_dir(app)?))
}

fn apply_retention(dir: &std::path::Path, settings: &HistorySettings) {
    if let Err(e) = history::prune(dir, settings, history::now_ms()) {
        eprintln!("[history] prune failed: {e}");
    }
}

/// Boot-time: tell core where history lives, hydrate its settings
/// cache so the first dictation follows the saved switch, and remove
/// text that aged out while the app was closed.
pub fn init(app: &AppHandle) {
    let (Ok(path), Ok(default)) = (settings_path(app), default_dir(app)) else {
        return;
    };
    history::set_default_dir(default.clone());
    let settings = core_settings::load_history_settings(&path);
    apply_retention(&history::resolve_dir(&settings, &default), &settings);
}

/// Fresh read from disk: the block is also edited by the headless CLI.
#[tauri::command]
pub fn history_get(app: AppHandle) -> Result<HistoryState, String> {
    let path = settings_path(&app)?;
    let default = default_dir(&app)?;
    let settings = core_settings::load_history_settings(&path);
    Ok(HistoryState {
        folder: history::resolve_dir(&settings, &default).display().to_string(),
        default_folder: default.display().to_string(),
        settings,
    })
}

/// Save the block; returns the new state (core trims and clamps). A
/// shorter retention takes effect at once. Files already written are
/// not moved when the folder changes.
#[tauri::command]
pub fn history_set(app: AppHandle, settings: HistorySettings) -> Result<HistoryState, String> {
    let path = settings_path(&app)?;
    let default = default_dir(&app)?;
    let saved = core_settings::save_history_settings(&path, settings)?;
    let dir = history::resolve_dir(&saved, &default);
    apply_retention(&dir, &saved);
    Ok(HistoryState {
        folder: dir.display().to_string(),
        default_folder: default.display().to_string(),
        settings: saved,
    })
}

/// Stored dictations, newest first, optionally filtered by `search`.
/// Same data as `openwhisper history list`. Reads files, so it runs
/// off the UI thread.
#[tauri::command]
pub async fn history_list(
    app: AppHandle,
    search: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<HistoryPage, String> {
    let dir = history_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        history::list(
            &dir,
            search.as_deref().unwrap_or(""),
            limit.unwrap_or(DEFAULT_PAGE),
            offset.unwrap_or(0),
        )
    })
    .await
    .map_err(|e| format!("history list: {e}"))
}

/// Remove one entry from its day file.
#[tauri::command]
pub fn history_delete(app: AppHandle, id: String) -> Result<bool, String> {
    history::delete(&history_dir(&app)?, &id).map_err(|e| e.to_string())
}

/// Remove every entry. Returns how many were removed.
#[tauri::command]
pub fn history_clear(app: AppHandle) -> Result<usize, String> {
    history::clear(&history_dir(&app)?).map_err(|e| e.to_string())
}

/// Open the history folder in Finder / Explorer. Creates it first so
/// there is something to open.
#[tauri::command]
pub fn history_reveal(app: AppHandle) -> Result<(), String> {
    let dir = history_dir(&app)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;

    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(target_os = "windows")]
    let program = "explorer";
    #[cfg(all(unix, not(target_os = "macos")))]
    let program = "xdg-open";

    std::process::Command::new(program)
        .arg(&dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("open {}: {e}", dir.display()))
}
