//! Modes commands (TASK-96.4). Thin wrappers: core owns the modes
//! block and every operation on it (`openwhisper_core::modes`). This
//! module resolves the settings path, saves what core hands back and
//! tells the rest of the app that the modes changed.
//!
//! A change can come from a command (the Modes pane, the select on
//! Home), from the switch-mode hotkey or from the tray menu. All three
//! go through [`change`], so after any of them core's cache holds the
//! saved block (the next recording reads it there), `modes_changed`
//! is emitted to every window and the tray menu is rebuilt.
//!
//! The same operations are reachable headless through
//! `openwhisper modes …`.

use std::path::Path;
use std::sync::Mutex;

use openwhisper_core::history;
use openwhisper_core::modes::{self, ModeEdit, ModeView};
use openwhisper_core::settings::{
    self as core_settings, AppDefault, BUILTIN_MODE_NAME, ModesSettings, RefineEffort,
    RefineSettings,
};
use tauri::{AppHandle, Emitter};

use crate::settings::settings_path;

/// Emitted to every window after the modes block changes. Payload:
/// [`ModesChanged`].
pub(crate) const MODES_CHANGED_EVENT: &str = "modes_changed";

/// The modes as they are now. What every change returns.
#[derive(serde::Serialize, Clone, Debug)]
pub struct ModesNow {
    /// The stored block, as core cleaned it.
    pub settings: ModesSettings,
    /// Every mode in the order it is listed and cycled, the built-in
    /// Dictation mode first.
    pub modes: Vec<ModeView>,
}

/// An app that can be given a default mode.
#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
pub struct ModeApp {
    /// See `openwhisper_core::focus::FocusContext::app_id`.
    pub id: String,
    /// Name to show a person; the id when no name is known.
    pub name: String,
}

/// What the Modes pane and the select on Home load: `settings`,
/// `modes` and `apps`, side by side.
#[derive(serde::Serialize)]
pub struct ModesState {
    #[serde(flatten)]
    pub now: ModesNow,
    /// Apps seen in the history, most recently used first, then apps
    /// that have a default mode but are not in the history.
    pub apps: Vec<ModeApp>,
}

/// Payload of `modes_changed`: `settings`, `modes` and `switched_to`,
/// side by side.
#[derive(serde::Serialize, Clone)]
pub struct ModesChanged {
    #[serde(flatten)]
    pub now: ModesNow,
    /// Name of the mode that is now selected, when the change was the
    /// user switching mode (the hotkey, the tray menu, the select on
    /// Home). The pill shows it for a moment. `None` for every other
    /// change: a mode added, edited or deleted, an app default.
    pub switched_to: Option<String>,
}

fn now(settings: ModesSettings, refine: &RefineSettings) -> ModesNow {
    ModesNow {
        modes: modes::list(&settings, refine),
        settings,
    }
}

fn selected_name(now: &ModesNow) -> String {
    now.modes
        .iter()
        .find(|m| m.id == now.settings.active)
        .map(|m| m.name.clone())
        .unwrap_or_else(|| BUILTIN_MODE_NAME.to_string())
}

/// Read the block from disk into core's cache. The headless CLI edits
/// the same block, so the cache can be behind; when it was, the tray
/// menu is rebuilt from what was read.
fn load_fresh(path: &Path) -> ModesSettings {
    let cached = core_settings::current_modes_settings();
    let settings = core_settings::load_modes_settings(path);
    if settings != cached {
        crate::tray::refresh();
    }
    settings
}

/// One change to the modes block: read it from disk, let `edit` (a
/// core function) make the new block, save it, then announce it.
/// `switch` says the change is the user switching mode. One change at
/// a time, so two quick presses of the hotkey are two steps and not
/// two reads of one state.
fn change(
    app: &AppHandle,
    switch: bool,
    edit: impl FnOnce(&ModesSettings) -> Result<ModesSettings, String>,
) -> Result<ModesNow, String> {
    static CHANGING: Mutex<()> = Mutex::new(());
    let _one_at_a_time = CHANGING.lock().unwrap_or_else(|e| e.into_inner());

    let path = settings_path(app)?;
    let current = load_fresh(&path);
    let next = edit(&current)?;
    // Saving also puts the block in core's cache.
    let saved = core_settings::save_modes_settings(&path, next)?;

    let now = now(saved, &core_settings::current_refine_settings());
    let payload = ModesChanged {
        switched_to: switch.then(|| selected_name(&now)),
        now: now.clone(),
    };
    if let Err(e) = app.emit(MODES_CHANGED_EVENT, payload) {
        eprintln!("{MODES_CHANGED_EVENT} emit failed: {e}");
    }
    crate::tray::refresh();
    Ok(now)
}

/// Boot-time: load the modes block into core's cache, so the first
/// recording runs in the saved mode and the tray menu lists the saved
/// modes.
pub fn init(app: &AppHandle) {
    if let Ok(path) = settings_path(app) {
        let _ = core_settings::load_modes_settings(&path);
    }
}

/// Switch to the next mode. Behind the switch-mode hotkey and the
/// `modes_cycle` command. One call, one step, recording or not. Does
/// file IO, so the hotkey backends call it from its own thread.
pub(crate) fn cycle(app: &AppHandle) -> Result<ModesNow, String> {
    change(app, true, |current| Ok(modes::cycle(current)))
}

/// Select a mode. Behind the tray menu and the `modes_select` command.
pub(crate) fn select(app: &AppHandle, id: &str) -> Result<ModesNow, String> {
    change(app, true, |current| modes::select(current, id))
}

/// The apps offered for a default mode: those seen in the history, in
/// that order, then those that have a default but are not in the
/// history (set from the CLI, or the history was cleared since).
fn merge_apps(seen: Vec<(String, String)>, defaults: &[AppDefault]) -> Vec<ModeApp> {
    let mut apps: Vec<ModeApp> = seen
        .into_iter()
        .map(|(id, name)| ModeApp { id, name })
        .collect();
    for default in defaults {
        if apps.iter().any(|a| a.id == default.app_id) {
            continue;
        }
        let name = default.app_name.trim();
        apps.push(ModeApp {
            id: default.app_id.clone(),
            name: if name.is_empty() { default.app_id.clone() } else { name.to_string() },
        });
    }
    apps
}

/// Fresh read from disk: the block is also edited by the headless
/// CLI. Reads the history files for the list of apps, so it runs off
/// the UI thread.
#[tauri::command]
pub async fn modes_get(app: AppHandle) -> Result<ModesState, String> {
    let path = settings_path(&app)?;
    let settings = load_fresh(&path);
    let refine = core_settings::load_refine_settings(&path);
    let seen = match crate::history::history_dir(&app) {
        Ok(dir) => tauri::async_runtime::spawn_blocking(move || history::apps(&dir))
            .await
            .map_err(|e| format!("modes apps: {e}"))?,
        Err(_) => Vec::new(),
    };
    Ok(ModesState {
        apps: merge_apps(seen, &settings.app_defaults),
        now: now(settings, &refine),
    })
}

/// Save the whole block; returns what core stored (ids made, names
/// trimmed, defaults for missing modes dropped).
#[tauri::command]
pub fn modes_save(app: AppHandle, settings: ModesSettings) -> Result<ModesNow, String> {
    change(&app, false, |_| Ok(settings))
}

/// Select the mode dictations run in when the app has no default.
#[tauri::command]
pub fn modes_select(app: AppHandle, id: String) -> Result<ModesNow, String> {
    select(&app, &id)
}

/// Same step as the switch-mode hotkey. The Windows in-window keyboard
/// fallback (`use-global-hotkey.ts`) calls it, because the keyboard
/// hook does not see keys pressed while our own window has focus.
#[tauri::command]
pub fn modes_cycle(app: AppHandle) -> Result<ModesNow, String> {
    cycle(&app)
}

/// Add a mode at the end of the list. Core makes its id from the name.
#[tauri::command]
pub fn modes_add(
    app: AppHandle,
    name: String,
    instructions: String,
    model: Option<String>,
    effort: Option<RefineEffort>,
) -> Result<ModesNow, String> {
    change(&app, false, |current| {
        modes::add(current, &name, &instructions, model.as_deref(), effort)
    })
}

/// Replace a mode's name, instructions, model and effort. `None` for
/// the model or the effort means the mode follows the Refine settings.
#[tauri::command]
pub fn modes_edit(
    app: AppHandle,
    id: String,
    name: String,
    instructions: String,
    model: Option<String>,
    effort: Option<RefineEffort>,
) -> Result<ModesNow, String> {
    let changes = ModeEdit {
        name: Some(name),
        instructions: Some(instructions),
        model: Some(model),
        effort: Some(effort),
    };
    change(&app, false, |current| modes::edit(current, &id, &changes))
}

/// Delete a mode. Apps that had it as their default lose that
/// default, and Dictation is selected if it was the selected mode.
#[tauri::command]
pub fn modes_delete(app: AppHandle, id: String) -> Result<ModesNow, String> {
    change(&app, false, |current| modes::delete(current, &id))
}

/// Give an app a default mode, replacing the one it had.
#[tauri::command]
pub fn modes_set_app_default(
    app: AppHandle,
    app_id: String,
    app_name: String,
    mode_id: String,
) -> Result<ModesNow, String> {
    change(&app, false, |current| {
        modes::set_app_default(current, &app_id, &app_name, &mode_id)
    })
}

/// Remove an app's default mode, so it follows the selected mode.
#[tauri::command]
pub fn modes_clear_app_default(app: AppHandle, app_id: String) -> Result<ModesNow, String> {
    change(&app, false, |current| modes::clear_app_default(current, &app_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use openwhisper_core::settings::Mode;

    fn app(id: &str, name: &str) -> ModeApp {
        ModeApp { id: id.into(), name: name.into() }
    }

    fn default_for(app_id: &str, app_name: &str, mode: &str) -> AppDefault {
        AppDefault { app_id: app_id.into(), app_name: app_name.into(), mode: mode.into() }
    }

    fn formal() -> ModesSettings {
        ModesSettings {
            active: "formal".into(),
            custom: vec![Mode {
                id: "formal".into(),
                name: "Formal".into(),
                instructions: "Write it formally.".into(),
                ..Mode::default()
            }],
            app_defaults: Vec::new(),
        }
    }

    #[test]
    fn apps_from_history_come_first_in_their_order() {
        let seen = vec![
            ("com.example.chat".to_string(), "Chatter".to_string()),
            ("com.example.mail".to_string(), "Mailbox".to_string()),
        ];
        let defaults = [default_for("com.example.mail", "Mail", "dictation")];
        assert_eq!(
            merge_apps(seen, &defaults),
            vec![app("com.example.chat", "Chatter"), app("com.example.mail", "Mailbox")]
        );
    }

    #[test]
    fn an_app_with_a_default_is_offered_without_any_history() {
        let defaults = [
            default_for("com.example.notes", "Notes", "dictation"),
            default_for("editor.exe", " ", "dictation"),
        ];
        assert_eq!(
            merge_apps(Vec::new(), &defaults),
            // No stored name: the id stands in for it.
            vec![app("com.example.notes", "Notes"), app("editor.exe", "editor.exe")]
        );
    }

    #[test]
    fn the_list_has_the_built_in_mode_first_and_names_the_selected_one() {
        let now = now(formal(), &RefineSettings::default());
        let ids: Vec<&str> = now.modes.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["dictation", "formal"]);
        assert!(now.modes[0].builtin);
        assert_eq!(selected_name(&now), "Formal");

        let none_selected = now_with_active("gone");
        assert_eq!(selected_name(&none_selected), BUILTIN_MODE_NAME);
    }

    fn now_with_active(active: &str) -> ModesNow {
        let mut settings = formal();
        settings.active = active.into();
        now(settings, &RefineSettings::default())
    }

    #[test]
    fn the_event_and_the_state_put_their_fields_side_by_side() {
        let now = now(formal(), &RefineSettings::default());
        let changed = serde_json::to_value(ModesChanged {
            switched_to: Some(selected_name(&now)),
            now: now.clone(),
        })
        .unwrap();
        assert_eq!(changed["settings"]["active"], "formal");
        assert_eq!(changed["modes"][1]["name"], "Formal");
        assert_eq!(changed["switched_to"], "Formal");

        let state = serde_json::to_value(ModesState {
            now,
            apps: vec![app("com.example.chat", "Chatter")],
        })
        .unwrap();
        assert_eq!(state["settings"]["custom"][0]["id"], "formal");
        assert_eq!(state["modes"][0]["id"], "dictation");
        assert_eq!(state["apps"][0]["name"], "Chatter");
    }
}
