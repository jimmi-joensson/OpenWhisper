//! Refine settings commands (TASK-93). Thin wrappers: core owns the
//! settings block, the backends, the keychain entry and the
//! connection test; this module resolves the settings path and moves
//! the slow calls (a CLI probe, a model call) off the UI thread.
//!
//! The same functions are reachable headless through
//! `openwhisper refine …`.

use std::sync::Arc;

use openwhisper_core::refine::history::{self, RefineRun, RefineSummary};
use openwhisper_core::refine::{self, BackendStatus, ConnectionTest, secrets};
use openwhisper_core::settings::{self as core_settings, RefineSettings};
use openwhisper_core::store::Store;
use tauri::AppHandle;

use crate::settings::settings_path;

/// What the Refine pane loads when it opens.
#[derive(serde::Serialize)]
pub struct RefineState {
    pub settings: RefineSettings,
    /// The built-in cleanup instructions, shown when the user has not
    /// written their own.
    pub default_instructions: &'static str,
    /// The sentence the connection test sends.
    pub test_sentence: &'static str,
}

/// Boot-time: hydrate core's Refine cache so the first dictation sees
/// the saved switch.
pub fn init(app: &AppHandle) {
    if let Ok(path) = settings_path(app) {
        let _ = core_settings::load_refine_settings(&path);
    }
}

/// Fresh read from disk: the block is also edited by the headless CLI.
#[tauri::command]
pub fn refine_get(app: AppHandle) -> Result<RefineState, String> {
    let path = settings_path(&app)?;
    Ok(RefineState {
        settings: core_settings::load_refine_settings(&path),
        default_instructions: refine::DEFAULT_INSTRUCTIONS,
        test_sentence: refine::TEST_SENTENCE,
    })
}

/// Save the whole block; returns what core stored (trimmed, clamped).
#[tauri::command]
pub fn refine_set(app: AppHandle, settings: RefineSettings) -> Result<RefineSettings, String> {
    let path = settings_path(&app)?;
    core_settings::save_refine_settings(&path, settings)
}

/// Can the selected backend run? Probes the Claude CLI (about 0.2 s),
/// so it runs on a blocking thread.
#[tauri::command]
pub async fn refine_status() -> Result<BackendStatus, String> {
    let settings = core_settings::current_refine_settings();
    tauri::async_runtime::spawn_blocking(move || refine::backend_status(&settings))
        .await
        .map_err(|e| format!("refine status: {e}"))
}

/// Send the test sentence through the selected backend. One model
/// call; takes as long as the backend does (bounded by the timeout).
#[tauri::command]
pub async fn refine_test() -> Result<ConnectionTest, String> {
    let settings = core_settings::current_refine_settings();
    tauri::async_runtime::spawn_blocking(move || refine::test_connection(&settings))
        .await
        .map_err(|e| format!("refine test: {e}"))
}

/// What the Diagnostics pane shows about Refine: numbers over the
/// newest runs, and the runs themselves (timings and reasons, never
/// any dictated text).
#[derive(serde::Serialize)]
pub struct RefineHistory {
    pub summary: RefineSummary,
    pub runs: Vec<RefineRun>,
}

/// Newest `limit` Refine runs with their summary. Same data as
/// `openwhisper refine history`.
#[tauri::command]
pub fn refine_history(
    store: tauri::State<'_, Arc<Store>>,
    limit: Option<u32>,
) -> Result<RefineHistory, String> {
    let runs = history::recent(&store, limit.unwrap_or(50)).map_err(|e| e.to_string())?;
    Ok(RefineHistory {
        summary: history::summarize(&runs),
        runs,
    })
}

/// Delete the stored Refine runs.
#[tauri::command]
pub fn refine_history_clear(store: tauri::State<'_, Arc<Store>>) -> Result<(), String> {
    history::clear(&store).map_err(|e| e.to_string())
}

/// Store the API key in the OS keychain. The key is never returned to
/// the webview and never written to a file. An empty key clears it.
#[tauri::command]
pub async fn refine_set_api_key(key: String) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || {
        secrets::set_api_key(&key)?;
        Ok(secrets::has_api_key())
    })
    .await
    .map_err(|e| format!("refine key: {e}"))?
}
