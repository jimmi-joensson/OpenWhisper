//! Transcript history commands (TASK-95). Thin wrappers: core owns
//! the settings block, the stored text and the retention rule; this
//! module resolves paths and hands the calls through.
//!
//! The same functions are reachable headless through
//! `openwhisper history …`.

use std::sync::Arc;

use openwhisper_core::history::{self, HistoryPage};
use openwhisper_core::settings::{self as core_settings, HistorySettings};
use openwhisper_core::store::Store;
use tauri::{AppHandle, Manager};

use crate::settings::settings_path;

/// How many entries the Home list loads per page.
const DEFAULT_PAGE: u32 = 30;

/// What Home and the Settings section load.
#[derive(serde::Serialize)]
pub struct HistoryState {
    pub settings: HistorySettings,
    /// Where the text is stored, shown to the user as-is. Empty when
    /// the path cannot be resolved.
    pub database: String,
}

fn apply_retention(store: &Store, settings: &HistorySettings) {
    if let Err(e) = history::prune(store, settings, history::now_ms()) {
        eprintln!("[history] prune failed: {e}");
    }
}

/// Boot-time: hydrate core's history cache so the first dictation
/// follows the saved switch, and remove text that aged out while the
/// app was closed. Call after the store is registered.
pub fn init(app: &AppHandle) {
    let Ok(path) = settings_path(app) else {
        return;
    };
    let settings = core_settings::load_history_settings(&path);
    if let Some(store) = app.try_state::<Arc<Store>>() {
        apply_retention(&store, &settings);
    }
}

/// Fresh read from disk: the block is also edited by the headless CLI.
#[tauri::command]
pub fn history_get(app: AppHandle) -> Result<HistoryState, String> {
    let path = settings_path(&app)?;
    let database = app
        .path()
        .app_data_dir()
        .map(|dir| dir.join("openwhisper.db").display().to_string())
        .unwrap_or_default();
    Ok(HistoryState {
        settings: core_settings::load_history_settings(&path),
        database,
    })
}

/// Save the block; returns what core stored (clamped). A shorter
/// retention takes effect at once.
#[tauri::command]
pub fn history_set(
    app: AppHandle,
    store: tauri::State<'_, Arc<Store>>,
    settings: HistorySettings,
) -> Result<HistorySettings, String> {
    let path = settings_path(&app)?;
    let saved = core_settings::save_history_settings(&path, settings)?;
    apply_retention(&store, &saved);
    Ok(saved)
}

/// Stored dictations, newest first, optionally filtered by `search`.
/// Same data as `openwhisper history list`.
#[tauri::command]
pub fn history_list(
    store: tauri::State<'_, Arc<Store>>,
    search: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<HistoryPage, String> {
    history::list(
        &store,
        search.as_deref().unwrap_or(""),
        limit.unwrap_or(DEFAULT_PAGE),
        offset.unwrap_or(0),
    )
    .map_err(|e| e.to_string())
}

/// Remove one entry's text. The stats row stays.
#[tauri::command]
pub fn history_delete(store: tauri::State<'_, Arc<Store>>, id: i64) -> Result<bool, String> {
    history::delete(&store, id).map_err(|e| e.to_string())
}

/// Remove all stored text. The stats rows stay. Returns how many
/// entries were removed.
#[tauri::command]
pub fn history_clear(store: tauri::State<'_, Arc<Store>>) -> Result<usize, String> {
    history::clear(&store).map_err(|e| e.to_string())
}
