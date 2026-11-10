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
//! Those are three threads, and the pane reads the block from a
//! fourth. Everything here that puts a block into core's cache does
//! it while holding one lock ([`modes_lock`]): a change (read, edit,
//! save) and a plain read ([`read_fresh`]). Without it a read that
//! started before a change could finish after it and leave the cache
//! holding the block from before the change, and the next recording
//! would run in the mode the user just switched away from.
//!
//! The same operations are reachable headless through
//! `openwhisper modes …`.

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use openwhisper_core::modes::{self, ModeApp, ModeEdit, ModeView};
use openwhisper_core::settings::{
    self as core_settings, BUILTIN_MODE_NAME, ModesSettings, RefineBackendKind, RefineEffort,
    RefineSettings,
};
use openwhisper_core::{focus, history};
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
    pub modes: Vec<ModeRow>,
}

/// A mode as the windows list it: core's view of it, plus what its
/// own model and effort mean with the Refine backend in use. Core
/// decides that (`ModeView::overrides_apply`); the windows only show
/// it.
#[derive(serde::Serialize, Clone, Debug)]
pub struct ModeRow {
    #[serde(flatten)]
    pub view: ModeView,
    /// The backend the mode's own model and effort were chosen for;
    /// `None` when it has neither.
    pub override_backend: Option<RefineBackendKind>,
    /// They are in use: that backend is the one Refine uses. `false`
    /// means the mode runs with the Refine model and effort, either
    /// because it has none of its own or because they are for the
    /// other backend.
    pub overrides_apply: bool,
}

/// What the Modes pane and the select on Home load: `settings`,
/// `modes` and `apps`, side by side.
#[derive(serde::Serialize)]
pub struct ModesState {
    #[serde(flatten)]
    pub now: ModesNow,
    /// The apps a default mode can be given to: the ones that have
    /// one, then the ones dictated into before (from the history,
    /// when it is on), then the ones that are open now
    /// (`modes::apps_for_defaults`).
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
    let modes = modes::list(&settings, refine)
        .into_iter()
        .map(|view| ModeRow {
            override_backend: view.override_backend(),
            overrides_apply: view.overrides_apply(refine.backend),
            view,
        })
        .collect();
    ModesNow { modes, settings }
}

fn selected_name(now: &ModesNow) -> String {
    now.modes
        .iter()
        .find(|m| m.view.id == now.settings.active)
        .map(|m| m.view.name.clone())
        .unwrap_or_else(|| BUILTIN_MODE_NAME.to_string())
}

/// The lock every read and change of the modes block in this process
/// holds while it touches core's cache. See the module docs.
fn modes_lock() -> MutexGuard<'static, ()> {
    static MODES: Mutex<()> = Mutex::new(());
    MODES.lock().unwrap_or_else(|e| e.into_inner())
}

/// Read the block from disk into core's cache. The headless CLI edits
/// the same block, so the cache can be behind; when it was, the tray
/// menu is rebuilt from what was read. The caller holds
/// [`modes_lock`].
fn load_fresh(_held: &MutexGuard<'static, ()>, path: &Path) -> ModesSettings {
    let cached = core_settings::current_modes_settings();
    let settings = core_settings::load_modes_settings(path);
    if settings != cached {
        crate::tray::refresh();
    }
    settings
}

/// [`load_fresh`] for a caller that only reads: takes the lock for
/// the read, so it cannot put a block from before a change into the
/// cache after that change.
fn read_fresh(path: &Path) -> ModesSettings {
    load_fresh(&modes_lock(), path)
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
    let one_at_a_time = modes_lock();

    let path = settings_path(app)?;
    let current = load_fresh(&one_at_a_time, &path);
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
        let _ = read_fresh(&path);
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

/// Fresh read from disk: the block is also edited by the headless
/// CLI. Reads the history files and asks the OS which apps are open
/// for the list of apps, so that part runs off the UI thread.
#[tauri::command]
pub async fn modes_get(app: AppHandle) -> Result<ModesState, String> {
    let path = settings_path(&app)?;
    let settings = read_fresh(&path);
    let refine = core_settings::load_refine_settings(&path);
    // History is off by default: then there is no folder to read and
    // the open apps are the whole list.
    let history_dir = crate::history::history_dir(&app).ok();
    let (seen, running) = tauri::async_runtime::spawn_blocking(move || {
        let seen = history_dir.map(|dir| history::apps(&dir)).unwrap_or_default();
        (seen, focus::running_apps())
    })
    .await
    .map_err(|e| format!("modes apps: {e}"))?;
    Ok(ModesState {
        apps: modes::apps_for_defaults(&settings, &seen, &running),
        now: now(settings, &refine),
    })
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

/// The backend a model or effort picked in the mode dialog is for:
/// the one Refine uses, since that is the backend whose models and
/// effort levels the dialog offered.
fn active_backend() -> RefineBackendKind {
    core_settings::current_refine_settings().backend
}

/// Add a mode at the end of the list. Core makes its id from the name
/// and refuses a name another mode has. A model or effort is stored
/// as chosen for the Refine backend in use.
#[tauri::command]
pub fn modes_add(
    app: AppHandle,
    name: String,
    instructions: String,
    model: Option<String>,
    effort: Option<RefineEffort>,
) -> Result<ModesNow, String> {
    change(&app, false, |current| {
        modes::add(current, &name, &instructions, model.as_deref(), effort, Some(active_backend()))
    })
}

/// Replace a mode's name, instructions, model and effort. `None` for
/// the model or the effort means the mode follows the Refine settings.
/// The model and effort are for the Refine backend in use; a mode
/// whose own ones are for the other backend keeps them unless this
/// edit names a model or an effort (`ModeEdit::backend`).
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
        backend: Some(active_backend()),
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
    fn a_row_says_whether_the_modes_own_model_is_in_use() {
        let mut settings = formal();
        settings.custom[0].model = Some("opus".into());
        settings.custom[0].backend = Some(RefineBackendKind::ClaudeCli);
        let claude = RefineSettings::default();
        let openai = RefineSettings { backend: RefineBackendKind::OpenaiCompatible, ..RefineSettings::default() };

        let rows = now(settings.clone(), &claude).modes;
        assert!(rows[1].overrides_apply);
        assert_eq!(rows[1].override_backend, Some(RefineBackendKind::ClaudeCli));
        // The built-in mode has nothing of its own.
        assert!(!rows[0].overrides_apply);
        assert_eq!(rows[0].override_backend, None);

        // The user switched Refine to the other backend: the mode's
        // own model is kept, and not in use.
        let rows = now(settings, &openai).modes;
        assert!(!rows[1].overrides_apply);
        let json = serde_json::to_value(&rows[1]).unwrap();
        assert_eq!(json["id"], "formal");
        assert_eq!(json["model"], "opus");
        assert_eq!(json["backend"], "claude-cli");
        assert_eq!(json["override_backend"], "claude-cli");
        assert_eq!(json["overrides_apply"], false);
    }

    /// A read (the Modes pane loading) next to changes (the hotkey
    /// cycling). Right after a change has saved, core's cache must
    /// hold what it saved; a read that does not take the lock can
    /// finish in between and put the block from before the change
    /// back.
    #[test]
    fn a_read_cannot_put_an_older_block_in_the_cache_after_a_change() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let mut block = formal();
        block.active = "dictation".into();
        core_settings::save_modes_settings(&path, block).unwrap();

        let done = AtomicBool::new(false);
        let stale = std::thread::scope(|s| {
            let readers: Vec<_> = (0..3)
                .map(|_| {
                    s.spawn(|| {
                        while !done.load(Ordering::Relaxed) {
                            let _ = read_fresh(&path);
                        }
                    })
                })
                .collect();
            let mut stale = 0;
            for _ in 0..3000 {
                // What `change` does, step for step.
                let held = modes_lock();
                let current = load_fresh(&held, &path);
                let saved = core_settings::save_modes_settings(&path, modes::cycle(&current)).unwrap();
                if core_settings::current_modes_settings() != saved {
                    stale += 1;
                }
            }
            done.store(true, Ordering::Relaxed);
            for reader in readers {
                reader.join().unwrap();
            }
            stale
        });
        assert_eq!(stale, 0, "the cache held an older block right after {stale} of 3000 changes");
        core_settings::set_modes_settings_cache(ModesSettings::default());
    }

    #[test]
    fn the_list_has_the_built_in_mode_first_and_names_the_selected_one() {
        let now = now(formal(), &RefineSettings::default());
        let ids: Vec<&str> = now.modes.iter().map(|m| m.view.id.as_str()).collect();
        assert_eq!(ids, ["dictation", "formal"]);
        assert!(now.modes[0].view.builtin);
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
