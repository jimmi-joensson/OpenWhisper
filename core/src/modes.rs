//! Modes (TASK-96): what the Refine step does with a dictation.
//!
//! A mode is a name plus instructions, optionally with its own model
//! and effort. One mode is built in, "Dictation": the cleanup Refine
//! has always done, with the instructions from the Refine settings.
//! The user makes the others ("Formal", "Write in English", "Prompt
//! for a coding agent"), and those rewrite the text on purpose.
//!
//! This module is the logic over the stored block
//! ([`crate::settings::ModesSettings`]) and nothing else: no disk, no
//! model call, no platform code. So it compiles in every flavor of the
//! crate, and the desktop app and `openwhisper modes` get the same
//! answers from the same functions:
//!
//! - [`list`]: the modes in the order they are shown and cycled, the
//!   built-in one first.
//! - [`resolve`]: which mode a dictation runs in. The frontmost app's
//!   default mode when it has one, otherwise the mode the user
//!   selected. The answer says which of the two it was.
//! - [`add`], [`edit`], [`delete`], [`select`], [`cycle`],
//!   [`set_app_default`], [`clear_app_default`]: each takes the block
//!   and returns a new, cleaned one. Saving it is the caller's step
//!   ([`crate::settings::save_modes_settings`]).
//! - [`apps_for_defaults`]: the apps a default mode can be given to.
//!
//! What a mode does to the prompt and the model call is in
//! `crate::refine` (feature `refine`).

use serde::{Deserialize, Serialize};

use crate::settings::{
    AppDefault, BUILTIN_MODE_ID, BUILTIN_MODE_NAME, MODES_MAX, Mode, ModesSettings, RefineBackendKind,
    RefineEffort, RefineSettings, app_key, mode_name_key,
};

/// A mode as the UI and the pipeline see it: a custom mode, or the
/// built-in one put together from the Refine settings.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ModeView {
    pub id: String,
    pub name: String,
    /// For the built-in mode: the Refine instructions, empty when the
    /// user wrote none (then the default cleanup applies).
    pub instructions: String,
    /// `None` = the model in the Refine settings. Always `None` for
    /// the built-in mode.
    pub model: Option<String>,
    /// `None` = the effort in the Refine settings. Always `None` for
    /// the built-in mode.
    pub effort: Option<RefineEffort>,
    /// The Refine backend `model` and `effort` were chosen for
    /// ([`Mode::backend`]). `None` without a model or effort, and for
    /// a block written before the field existed.
    pub backend: Option<RefineBackendKind>,
    /// The built-in Dictation mode: cannot be renamed or deleted.
    pub builtin: bool,
}

impl ModeView {
    /// The built-in mode for these Refine settings.
    pub fn builtin(refine: &RefineSettings) -> Self {
        Self {
            id: BUILTIN_MODE_ID.to_string(),
            name: BUILTIN_MODE_NAME.to_string(),
            instructions: refine.instructions.clone(),
            model: None,
            effort: None,
            backend: None,
            builtin: true,
        }
    }

    fn custom(mode: &Mode) -> Self {
        Self {
            id: mode.id.clone(),
            name: mode.name.clone(),
            instructions: mode.instructions.clone(),
            model: mode.model.clone(),
            effort: mode.effort,
            backend: mode.backend,
            builtin: false,
        }
    }

    /// The backend this mode's own model and effort are for, or
    /// `None` when it has neither. A model or effort stored without a
    /// backend (a block from before the field existed) is for the
    /// Claude CLI: that was the only backend with named models, and
    /// "opus" sent to an OpenAI-compatible server fails every
    /// dictation.
    pub fn override_backend(&self) -> Option<RefineBackendKind> {
        (self.model.is_some() || self.effort.is_some())
            .then(|| self.backend.unwrap_or(RefineBackendKind::ClaudeCli))
    }

    /// The mode's own model and effort are used while `active` is the
    /// Refine backend. `false` also when it has neither: then there
    /// is nothing to apply and the Refine settings stand.
    pub fn overrides_apply(&self, active: RefineBackendKind) -> bool {
        self.override_backend() == Some(active)
    }

    /// The mode rewrites the text on purpose: it is a custom mode with
    /// instructions of its own. A custom mode without instructions
    /// cleans up like Dictation and only swaps the model or effort.
    pub fn rewrites(&self) -> bool {
        !self.builtin && !self.instructions.trim().is_empty()
    }
}

/// Why a mode was the one that ran.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ModeReason {
    /// It is the mode the user selected.
    #[default]
    Selected,
    /// The frontmost app has it as its default.
    AppDefault,
}

impl ModeReason {
    /// Stable id, same as the serialized form.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Selected => "selected",
            Self::AppDefault => "app-default",
        }
    }
}

/// The mode a dictation runs in, and why.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ModeChoice {
    pub mode: ModeView,
    pub reason: ModeReason,
    /// The app whose default this is. Empty when the mode is simply
    /// the selected one.
    pub app_id: String,
    /// Name of that app to show a person; the id when no name is
    /// known. Empty when the mode is simply the selected one.
    pub app_name: String,
}

/// Every mode, in the order they are shown and cycled: the built-in
/// one first, then the user's own.
pub fn list(modes: &ModesSettings, refine: &RefineSettings) -> Vec<ModeView> {
    let mut out = vec![ModeView::builtin(refine)];
    out.extend(modes.custom.iter().map(ModeView::custom));
    out
}

/// One mode by id.
pub fn get(modes: &ModesSettings, refine: &RefineSettings, id: &str) -> Option<ModeView> {
    if id == BUILTIN_MODE_ID {
        return Some(ModeView::builtin(refine));
    }
    modes.custom_mode(id).map(ModeView::custom)
}

/// The mode for a dictation into `app` (id and name of the frontmost
/// app; `None` or an empty id when it is not known). An app with a
/// default mode gets that one; every other app gets the selected mode.
///
/// Never fails: a selection or a default that points at a mode which
/// no longer exists resolves to Dictation.
pub fn resolve(modes: &ModesSettings, refine: &RefineSettings, app: Option<(&str, &str)>) -> ModeChoice {
    if let Some((app_id, app_name)) = app {
        let app_id = app_id.trim();
        // Ids are compared in lower case on both sides, so a block
        // that was never cleaned matches too.
        let key = app_key(app_id);
        let default = modes.app_defaults.iter().find(|d| !key.is_empty() && app_key(&d.app_id) == key);
        if let Some(default) = default
            && let Some(mode) = get(modes, refine, &default.mode)
        {
            let name = [app_name.trim(), default.app_name.trim(), app_id]
                .into_iter()
                .find(|n| !n.is_empty())
                .unwrap_or_default();
            return ModeChoice {
                mode,
                reason: ModeReason::AppDefault,
                app_id: app_id.to_string(),
                app_name: name.to_string(),
            };
        }
    }
    ModeChoice {
        mode: get(modes, refine, &modes.active).unwrap_or_else(|| ModeView::builtin(refine)),
        reason: ModeReason::Selected,
        app_id: String::new(),
        app_name: String::new(),
    }
}

/// Changes to one custom mode. A field left `None` stays as it is;
/// for the model and the effort, `Some(None)` removes the mode's own
/// value so it follows the Refine settings again.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModeEdit {
    pub name: Option<String>,
    pub instructions: Option<String>,
    pub model: Option<Option<String>>,
    pub effort: Option<Option<RefineEffort>>,
    /// The Refine backend the model and effort in this edit are for:
    /// the one in use where they were picked. Pass it whenever
    /// `model` or `effort` is set.
    ///
    /// It matters when the mode already has a model or effort for
    /// *another* backend. Those are not what the person editing saw
    /// (the app shows "Same as Refine" for them), so an edit that
    /// names no model and no effort leaves them alone, and an edit
    /// that names one replaces both: a model for one backend and an
    /// effort for the other cannot be stored together.
    pub backend: Option<RefineBackendKind>,
}

fn unknown(id: &str) -> String {
    format!("There is no mode with the id \"{id}\".")
}

/// Refuse a name another mode already has, ignoring case and outer
/// space. `except` is the id of the mode being renamed.
fn check_name_free(modes: &ModesSettings, name: &str, except: Option<&str>) -> Result<(), String> {
    let key = mode_name_key(name);
    let holder = if key == mode_name_key(BUILTIN_MODE_NAME) {
        Some(BUILTIN_MODE_NAME)
    } else {
        modes
            .custom
            .iter()
            .find(|m| Some(m.id.as_str()) != except && mode_name_key(&m.name) == key)
            .map(|m| m.name.as_str())
    };
    match holder {
        Some(taken) => Err(format!("There is already a mode called \"{taken}\". Give this one another name.")),
        None => Ok(()),
    }
}

/// Add a custom mode at the end of the list. Its id is made from the
/// name; read it from the last entry of `custom` in the result.
///
/// `backend` is the Refine backend `model` and `effort` were chosen
/// for (the one in use); it is stored only when the mode has one of
/// them. The name must not be another mode's name, ignoring case.
pub fn add(
    modes: &ModesSettings,
    name: &str,
    instructions: &str,
    model: Option<&str>,
    effort: Option<RefineEffort>,
    backend: Option<RefineBackendKind>,
) -> Result<ModesSettings, String> {
    if name.trim().is_empty() {
        return Err("A mode needs a name.".to_string());
    }
    if modes.custom.len() >= MODES_MAX {
        return Err(format!("There are already {MODES_MAX} modes. Delete one before adding another."));
    }
    check_name_free(modes, name, None)?;
    let mut next = modes.clone();
    next.custom.push(Mode {
        id: String::new(),
        name: name.to_string(),
        instructions: instructions.to_string(),
        model: model.map(str::to_string),
        effort,
        backend,
    });
    Ok(next.cleaned())
}

/// Change a custom mode. Its id stays, also when it is renamed.
pub fn edit(modes: &ModesSettings, id: &str, changes: &ModeEdit) -> Result<ModesSettings, String> {
    if id == BUILTIN_MODE_ID {
        return Err(format!(
            "The {BUILTIN_MODE_NAME} mode is built in. It cannot be renamed, and its instructions are the Refine instructions."
        ));
    }
    let mut next = modes.clone();
    let Some(mode) = next.custom.iter_mut().find(|m| m.id == id) else {
        return Err(unknown(id));
    };
    if let Some(name) = &changes.name {
        if name.trim().is_empty() {
            return Err("A mode needs a name.".to_string());
        }
        check_name_free(modes, name, Some(id))?;
        mode.name = name.clone();
    }
    if let Some(instructions) = &changes.instructions {
        mode.instructions = instructions.clone();
    }
    let names_a_value = matches!(changes.model, Some(Some(_))) || matches!(changes.effort, Some(Some(_)));
    let kept_for_another_backend = match (changes.backend, mode.override_backend()) {
        (Some(edit_for), Some(stored_for)) => edit_for != stored_for,
        _ => false,
    };
    if kept_for_another_backend {
        // See `ModeEdit::backend`.
        if names_a_value {
            mode.model = changes.model.clone().flatten();
            mode.effort = changes.effort.flatten();
            mode.backend = changes.backend;
        }
    } else {
        if let Some(model) = &changes.model {
            mode.model = model.clone();
        }
        if let Some(effort) = changes.effort {
            mode.effort = effort;
        }
        if names_a_value && changes.backend.is_some() {
            mode.backend = changes.backend;
        }
    }
    Ok(next.cleaned())
}

/// Delete a custom mode. Apps that had it as their default lose that
/// default, and if it was the selected mode, Dictation is selected.
pub fn delete(modes: &ModesSettings, id: &str) -> Result<ModesSettings, String> {
    if id == BUILTIN_MODE_ID {
        return Err(format!("The {BUILTIN_MODE_NAME} mode is built in and cannot be deleted."));
    }
    if modes.custom_mode(id).is_none() {
        return Err(unknown(id));
    }
    let mut next = modes.clone();
    next.custom.retain(|m| m.id != id);
    // `cleaned` drops the defaults that pointed at it and moves the
    // selection back to Dictation.
    Ok(next.cleaned())
}

/// Select the mode dictations run in when the app has no default.
pub fn select(modes: &ModesSettings, id: &str) -> Result<ModesSettings, String> {
    if !modes.has_mode(id) {
        return Err(unknown(id));
    }
    let mut next = modes.clone();
    next.active = id.to_string();
    Ok(next.cleaned())
}

/// Select the next mode in list order, wrapping around after the
/// last. With only the built-in mode the selection stays where it is.
pub fn cycle(modes: &ModesSettings) -> ModesSettings {
    let mut next = modes.clone().cleaned();
    let ids: Vec<&str> = std::iter::once(BUILTIN_MODE_ID)
        .chain(next.custom.iter().map(|m| m.id.as_str()))
        .collect();
    let at = ids.iter().position(|id| *id == next.active).unwrap_or(0);
    next.active = ids[(at + 1) % ids.len()].to_string();
    next
}

/// Give an app a default mode, replacing the one it had. `app_name`
/// may be empty; then the name the app was stored under before is
/// kept.
pub fn set_app_default(
    modes: &ModesSettings,
    app_id: &str,
    app_name: &str,
    mode_id: &str,
) -> Result<ModesSettings, String> {
    let app_id = app_id.trim();
    if app_id.is_empty() {
        return Err("An app default needs the id of the app.".to_string());
    }
    if !modes.has_mode(mode_id) {
        return Err(unknown(mode_id));
    }
    let mut next = modes.clone();
    let app_id = app_key(app_id);
    let known_name = next
        .app_defaults
        .iter()
        .find(|d| app_key(&d.app_id) == app_id)
        .map(|d| d.app_name.clone())
        .unwrap_or_default();
    let app_name = if app_name.trim().is_empty() { known_name } else { app_name.trim().to_string() };
    next.app_defaults.push(AppDefault { app_id, app_name, mode: mode_id.to_string() });
    Ok(next.cleaned())
}

/// Remove an app's default mode, so it follows the selected mode
/// again.
pub fn clear_app_default(modes: &ModesSettings, app_id: &str) -> Result<ModesSettings, String> {
    let app_id = app_id.trim();
    let key = app_key(app_id);
    if !modes.app_defaults.iter().any(|d| app_key(&d.app_id) == key) {
        return Err(format!("The app \"{app_id}\" has no default mode."));
    }
    let mut next = modes.clone();
    next.app_defaults.retain(|d| app_key(&d.app_id) != key);
    Ok(next.cleaned())
}

/// An app that can be given a default mode.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ModeApp {
    /// The app id in the form defaults are stored in
    /// ([`crate::settings::app_key`]: lower case), so it can be
    /// compared with [`AppDefault::app_id`] as is.
    pub id: String,
    /// Name to show a person, as the app gives it; the id when no
    /// name is known.
    pub name: String,
}

/// The apps offered for a default mode, each once:
///
/// 1. the apps that have a default, in stored order (also when they
///    are neither running nor in the history: set from the CLI, or
///    the history was cleared since);
/// 2. the apps dictated into before, as the history lists them
///    ([`crate::history::apps`], most recent first);
/// 3. the apps that are open now (`crate::focus::running_apps`).
///
/// `history` and `running` are `(app_id, app_name)` pairs. Ids are
/// compared ignoring case; the first place an app turns up decides
/// its position. Its name is the first one known in that order, and
/// the id when there is none.
pub fn apps_for_defaults(
    modes: &ModesSettings,
    history: &[(String, String)],
    running: &[(String, String)],
) -> Vec<ModeApp> {
    let mut apps: Vec<ModeApp> = Vec::new();
    let defaults = modes.app_defaults.iter().map(|d| (d.app_id.as_str(), d.app_name.as_str()));
    let seen = history.iter().chain(running).map(|(id, name)| (id.as_str(), name.as_str()));
    for (id, name) in defaults.chain(seen) {
        let id = app_key(id);
        let name = name.trim();
        if id.is_empty() {
            continue;
        }
        match apps.iter_mut().find(|a| a.id == id) {
            // Listed already: only a missing name is filled in.
            Some(known) => {
                if known.name == known.id && !name.is_empty() {
                    known.name = name.to_string();
                }
            }
            None => apps.push(ModeApp {
                name: if name.is_empty() { id.clone() } else { name.to_string() },
                id,
            }),
        }
    }
    apps
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLAUDE: Option<RefineBackendKind> = Some(RefineBackendKind::ClaudeCli);
    const OPENAI: Option<RefineBackendKind> = Some(RefineBackendKind::OpenaiCompatible);

    fn refine() -> RefineSettings {
        RefineSettings { instructions: "Use British spelling.".into(), ..RefineSettings::default() }
    }

    /// Dictation plus "Formal" and "Write in English".
    fn three() -> ModesSettings {
        let m = add(&ModesSettings::default(), "Formal", "Rewrite it in a formal tone.", None, None, None).unwrap();
        add(&m, "Write in English", "Write it in English.", Some("opus"), Some(RefineEffort::Medium), CLAUDE).unwrap()
    }

    #[test]
    fn list_starts_with_the_built_in_mode() {
        let modes = list(&three(), &refine());
        let names: Vec<&str> = modes.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["Dictation", "Formal", "Write in English"]);
        assert!(modes[0].builtin);
        assert_eq!(modes[0].id, BUILTIN_MODE_ID);
        // The built-in mode's instructions are the Refine instructions.
        assert_eq!(modes[0].instructions, "Use British spelling.");
        assert_eq!((modes[0].model.clone(), modes[0].effort), (None, None));
        assert!(!modes[1].builtin);
        assert_eq!(modes[2].id, "write-in-english");
        assert_eq!(modes[2].model.as_deref(), Some("opus"));
        assert_eq!(modes[2].effort, Some(RefineEffort::Medium));
    }

    #[test]
    fn only_a_custom_mode_with_instructions_rewrites() {
        let modes = add(&three(), "Careful", "  ", Some("opus"), None, CLAUDE).unwrap();
        let views = list(&modes, &refine());
        assert!(!views[0].rewrites(), "Dictation cleans up");
        assert!(views[1].rewrites());
        assert!(!views[3].rewrites(), "no instructions: Dictation with another model");
    }

    #[test]
    fn resolve_gives_the_selected_mode_and_says_so() {
        let modes = select(&three(), "formal").unwrap();
        for app in [None, Some(("", "")), Some(("com.example.notes", "Notes"))] {
            let choice = resolve(&modes, &refine(), app);
            assert_eq!(choice.mode.id, "formal");
            assert_eq!(choice.reason, ModeReason::Selected);
            assert_eq!((choice.app_id.as_str(), choice.app_name.as_str()), ("", ""));
        }
        // Nothing selected yet: Dictation.
        let choice = resolve(&ModesSettings::default(), &refine(), None);
        assert!(choice.mode.builtin);
        assert_eq!(choice.reason, ModeReason::Selected);
    }

    #[test]
    fn resolve_puts_the_app_default_before_the_selected_mode() {
        let modes = select(&three(), "formal").unwrap();
        let modes = set_app_default(&modes, "com.example.mail", "Mail", "write-in-english").unwrap();
        let modes = set_app_default(&modes, "com.example.terminal", "", BUILTIN_MODE_ID).unwrap();

        let choice = resolve(&modes, &refine(), Some(("com.example.mail", "Mail Pro")));
        assert_eq!(choice.mode.id, "write-in-english");
        assert_eq!(choice.reason, ModeReason::AppDefault);
        assert_eq!(choice.app_id, "com.example.mail");
        // The name the app has right now wins over the stored one.
        assert_eq!(choice.app_name, "Mail Pro");
        // Without a current name, the stored one; without either, the id.
        assert_eq!(resolve(&modes, &refine(), Some(("com.example.mail", ""))).app_name, "Mail");
        let choice = resolve(&modes, &refine(), Some(("com.example.terminal", "")));
        assert_eq!(choice.app_name, "com.example.terminal");
        // The built-in mode can be an app's default while another mode
        // is selected.
        assert!(choice.mode.builtin);
        assert_eq!(choice.reason, ModeReason::AppDefault);

        // Another app: the selected mode.
        let choice = resolve(&modes, &refine(), Some(("com.example.notes", "Notes")));
        assert_eq!((choice.mode.id.as_str(), choice.reason), ("formal", ModeReason::Selected));
    }

    #[test]
    fn resolve_survives_a_block_that_was_not_cleaned() {
        let odd = ModesSettings {
            active: "gone".into(),
            custom: vec![],
            app_defaults: vec![AppDefault {
                app_id: "com.example.mail".into(),
                app_name: "Mail".into(),
                mode: "also-gone".into(),
            }],
        };
        let choice = resolve(&odd, &refine(), Some(("com.example.mail", "Mail")));
        assert!(choice.mode.builtin);
        assert_eq!(choice.reason, ModeReason::Selected);
    }

    #[test]
    fn reasons_serialize_as_their_ids() {
        for r in [ModeReason::Selected, ModeReason::AppDefault] {
            assert_eq!(serde_json::to_string(&r).unwrap(), format!("\"{}\"", r.as_str()));
        }
        assert_eq!(ModeReason::AppDefault.as_str(), "app-default");
    }

    #[test]
    fn add_makes_an_id_from_the_name_and_refuses_an_empty_name() {
        let modes = three();
        assert_eq!(modes.custom[0].id, "formal");
        // The id "formal" stays with the renamed mode, so a new mode
        // called Formal gets the next one.
        let renamed =
            edit(&modes, "formal", &ModeEdit { name: Some("Business tone".into()), ..ModeEdit::default() }).unwrap();
        let again = add(&renamed, "Formal", "Another one.", None, None, None).unwrap();
        assert_eq!(again.custom.last().unwrap().id, "formal-2");
        // Selecting nothing new: adding does not change the selection.
        assert_eq!(again.active, BUILTIN_MODE_ID);
        assert_eq!(add(&modes, "   ", "x", None, None, None).unwrap_err(), "A mode needs a name.");

        let mut full = ModesSettings::default();
        for i in 0..MODES_MAX {
            full = add(&full, &format!("Mode {i}"), "x", None, None, None).unwrap();
        }
        let err = add(&full, "One too many", "x", None, None, None).unwrap_err();
        assert!(err.contains("already 50 modes"), "{err}");
    }

    #[test]
    fn edit_changes_fields_and_keeps_the_id() {
        let modes = select(&three(), "formal").unwrap();
        let modes = set_app_default(&modes, "com.example.mail", "Mail", "formal").unwrap();
        let renamed = edit(
            &modes,
            "formal",
            &ModeEdit {
                name: Some("Business tone".into()),
                model: Some(Some("haiku".into())),
                effort: Some(Some(RefineEffort::High)),
                ..ModeEdit::default()
            },
        )
        .unwrap();
        let mode = renamed.custom_mode("formal").unwrap();
        assert_eq!(mode.name, "Business tone");
        assert_eq!(mode.instructions, "Rewrite it in a formal tone.");
        assert_eq!(mode.model.as_deref(), Some("haiku"));
        assert_eq!(mode.effort, Some(RefineEffort::High));
        // Selection and app default still point at it.
        assert_eq!(renamed.active, "formal");
        assert_eq!(renamed.app_defaults[0].mode, "formal");

        // Model and effort can be handed back to the Refine settings.
        let cleared = edit(
            &renamed,
            "formal",
            &ModeEdit { model: Some(None), effort: Some(None), ..ModeEdit::default() },
        )
        .unwrap();
        let mode = cleared.custom_mode("formal").unwrap();
        assert_eq!((mode.model.clone(), mode.effort), (None, None));

        let err = edit(&modes, "formal", &ModeEdit { name: Some(" ".into()), ..ModeEdit::default() }).unwrap_err();
        assert_eq!(err, "A mode needs a name.");
        let err = edit(&modes, "nope", &ModeEdit::default()).unwrap_err();
        assert_eq!(err, "There is no mode with the id \"nope\".");
    }

    #[test]
    fn the_built_in_mode_cannot_be_edited_or_deleted() {
        let modes = three();
        let err = edit(&modes, BUILTIN_MODE_ID, &ModeEdit { name: Some("Mine".into()), ..ModeEdit::default() })
            .unwrap_err();
        assert!(err.contains("built in") && err.contains("cannot be renamed"), "{err}");
        let err = delete(&modes, BUILTIN_MODE_ID).unwrap_err();
        assert!(err.contains("built in") && err.contains("cannot be deleted"), "{err}");
    }

    #[test]
    fn delete_removes_the_modes_app_defaults_and_falls_back_to_dictation() {
        let modes = select(&three(), "formal").unwrap();
        let modes = set_app_default(&modes, "com.example.mail", "Mail", "formal").unwrap();
        let modes = set_app_default(&modes, "com.example.chat", "Chat", "write-in-english").unwrap();

        let after = delete(&modes, "formal").unwrap();
        assert!(after.custom_mode("formal").is_none());
        assert_eq!(after.active, BUILTIN_MODE_ID);
        let apps: Vec<&str> = after.app_defaults.iter().map(|d| d.app_id.as_str()).collect();
        assert_eq!(apps, ["com.example.chat"]);

        // Deleting a mode that is not selected leaves the selection.
        let after = delete(&modes, "write-in-english").unwrap();
        assert_eq!(after.active, "formal");
        assert_eq!(delete(&modes, "nope").unwrap_err(), "There is no mode with the id \"nope\".");
    }

    #[test]
    fn select_takes_any_existing_mode() {
        let modes = three();
        assert_eq!(select(&modes, "write-in-english").unwrap().active, "write-in-english");
        assert_eq!(select(&modes, BUILTIN_MODE_ID).unwrap().active, BUILTIN_MODE_ID);
        assert!(select(&modes, "nope").is_err());
    }

    #[test]
    fn cycle_walks_the_list_and_wraps() {
        let mut modes = three();
        let mut seen = Vec::new();
        for _ in 0..4 {
            modes = cycle(&modes);
            seen.push(modes.active.clone());
        }
        assert_eq!(seen, ["formal", "write-in-english", "dictation", "formal"]);
        // Only the built-in mode: nothing to cycle to.
        assert_eq!(cycle(&ModesSettings::default()).active, BUILTIN_MODE_ID);
    }

    #[test]
    fn app_defaults_are_set_replaced_and_cleared() {
        let modes = three();
        let modes = set_app_default(&modes, " com.example.mail ", "Mail", "formal").unwrap();
        // Setting it again replaces it; an empty name keeps the old one.
        let modes = set_app_default(&modes, "com.example.mail", "", "write-in-english").unwrap();
        assert_eq!(
            modes.app_defaults,
            vec![AppDefault {
                app_id: "com.example.mail".into(),
                app_name: "Mail".into(),
                mode: "write-in-english".into()
            }]
        );
        assert!(set_app_default(&modes, "  ", "x", "formal").is_err());
        assert!(set_app_default(&modes, "com.example.chat", "Chat", "nope").is_err());

        let cleared = clear_app_default(&modes, "com.example.mail").unwrap();
        assert!(cleared.app_defaults.is_empty());
        let err = clear_app_default(&cleared, "com.example.mail").unwrap_err();
        assert_eq!(err, "The app \"com.example.mail\" has no default mode.");
    }

    #[test]
    fn a_name_another_mode_has_is_refused_ignoring_case_and_space() {
        let modes = three();
        let taken = "There is already a mode called \"Formal\". Give this one another name.";
        for name in ["Formal", "formal", "  FORMAL  "] {
            assert_eq!(add(&modes, name, "x", None, None, None).unwrap_err(), taken, "{name}");
        }
        // The built-in mode's name is taken too.
        let err = add(&modes, " dictation ", "x", None, None, None).unwrap_err();
        assert_eq!(err, "There is already a mode called \"Dictation\". Give this one another name.");

        // Renaming onto another mode's name is refused the same way.
        let rename = |to: &str| ModeEdit { name: Some(to.into()), ..ModeEdit::default() };
        assert_eq!(edit(&modes, "write-in-english", &rename("FORMAL")).unwrap_err(), taken);
        assert!(edit(&modes, "formal", &rename("Dictation")).unwrap_err().contains("\"Dictation\""));
        // A mode can keep its own name, also with another case.
        assert_eq!(edit(&modes, "formal", &rename("Formal")).unwrap(), modes);
        assert_eq!(edit(&modes, "formal", &rename("FORMAL")).unwrap().custom[0].name, "FORMAL");
        // Nothing was stored by a refused call.
        assert_eq!(modes, three());
    }

    #[test]
    fn app_defaults_match_whatever_the_case_of_the_id() {
        let modes = three();
        let modes = set_app_default(&modes, "com.example.Mail", "Mail", "formal").unwrap();
        // Stored in lower case; the name is kept as given.
        assert_eq!(modes.app_defaults[0].app_id, "com.example.mail");
        assert_eq!(modes.app_defaults[0].app_name, "Mail");
        for id in ["com.example.Mail", "com.example.mail", "COM.EXAMPLE.MAIL", " com.example.MAIL "] {
            let choice = resolve(&modes, &refine(), Some((id, "")));
            assert_eq!((choice.mode.id.as_str(), choice.reason), ("formal", ModeReason::AppDefault), "{id}");
            assert_eq!(choice.app_name, "Mail");
        }
        // Another spelling replaces the default, it does not add one.
        let modes = set_app_default(&modes, "COM.EXAMPLE.MAIL", "", "write-in-english").unwrap();
        assert_eq!(modes.app_defaults.len(), 1);
        assert_eq!(modes.app_defaults[0].app_name, "Mail");
        assert_eq!(modes.app_defaults[0].mode, "write-in-english");
        // And another spelling clears it.
        assert!(clear_app_default(&modes, "Com.Example.Mail").unwrap().app_defaults.is_empty());

        // A block that was never cleaned (mixed case in the file) matches too.
        let raw = ModesSettings {
            app_defaults: vec![AppDefault { app_id: "Editor.EXE".into(), app_name: String::new(), mode: "formal".into() }],
            ..three()
        };
        assert_eq!(resolve(&raw, &refine(), Some(("editor.exe", "Editor"))).mode.id, "formal");
    }

    #[test]
    fn a_models_backend_is_stored_with_it() {
        let modes = add(&ModesSettings::default(), "Plain", "x", None, None, OPENAI).unwrap();
        assert_eq!(modes.custom[0].backend, None, "nothing of its own: no backend");
        let modes = add(&modes, "Small", "x", Some("small-model"), None, OPENAI).unwrap();
        assert_eq!(modes.custom[1].backend, OPENAI);
        let views = list(&modes, &refine());
        assert_eq!(views[2].override_backend(), OPENAI);
        assert!(views[2].overrides_apply(RefineBackendKind::OpenaiCompatible));
        assert!(!views[2].overrides_apply(RefineBackendKind::ClaudeCli));
        assert!(!views[1].overrides_apply(RefineBackendKind::ClaudeCli), "nothing to apply");
        assert!(!views[0].overrides_apply(RefineBackendKind::ClaudeCli), "the built-in mode has none");

        // Stored before the field existed: for the Claude CLI.
        let old = ModesSettings {
            custom: vec![Mode { name: "Old".into(), model: Some("opus".into()), ..Mode::default() }],
            ..ModesSettings::default()
        }
        .cleaned();
        let view = &list(&old, &refine())[1];
        assert_eq!(view.backend, None);
        assert_eq!(view.override_backend(), CLAUDE);
        assert!(!view.overrides_apply(RefineBackendKind::OpenaiCompatible));
    }

    #[test]
    fn an_edit_for_another_backend_replaces_or_leaves_the_stored_model_and_effort() {
        let modes = add(&ModesSettings::default(), "Small", "x", Some("small-model"), Some(RefineEffort::Minimal), OPENAI)
            .unwrap();
        let own = |m: &ModesSettings| {
            let mode = m.custom_mode("small").unwrap();
            (mode.model.clone(), mode.effort, mode.backend)
        };

        // Edited while the Claude CLI is in use, where the dialog showed
        // "Same as Refine" for both: a rename does not cost the mode its
        // OpenAI model.
        let renamed = edit(
            &modes,
            "small",
            &ModeEdit {
                name: Some("Tiny".into()),
                model: Some(None),
                effort: Some(None),
                backend: CLAUDE,
                ..ModeEdit::default()
            },
        )
        .unwrap();
        assert_eq!(renamed.custom[0].name, "Tiny");
        assert_eq!(own(&renamed), (Some("small-model".into()), Some(RefineEffort::Minimal), OPENAI));

        // Picking an effort there replaces both: the model was for the
        // other backend and does not carry over.
        let picked = edit(
            &modes,
            "small",
            &ModeEdit { model: Some(None), effort: Some(Some(RefineEffort::High)), backend: CLAUDE, ..ModeEdit::default() },
        )
        .unwrap();
        assert_eq!(own(&picked), (None, Some(RefineEffort::High), CLAUDE));
        let picked = edit(
            &modes,
            "small",
            &ModeEdit { model: Some(Some("opus".into())), backend: CLAUDE, ..ModeEdit::default() },
        )
        .unwrap();
        assert_eq!(own(&picked), (Some("opus".into()), None, CLAUDE));

        // The same backend: fields change one by one, as before.
        let same = edit(
            &modes,
            "small",
            &ModeEdit { effort: Some(Some(RefineEffort::Low)), backend: OPENAI, ..ModeEdit::default() },
        )
        .unwrap();
        assert_eq!(own(&same), (Some("small-model".into()), Some(RefineEffort::Low), OPENAI));
        // Without a backend in the edit (a clear from the CLI), the
        // stored values are the ones meant.
        let cleared =
            edit(&modes, "small", &ModeEdit { model: Some(None), effort: Some(None), ..ModeEdit::default() }).unwrap();
        assert_eq!(own(&cleared), (None, None, None));

        // An old block's model gets its backend on the next edit that
        // names one.
        let old = ModesSettings {
            custom: vec![Mode { name: "Old".into(), model: Some("opus".into()), ..Mode::default() }],
            ..ModesSettings::default()
        }
        .cleaned();
        let touched = edit(
            &old,
            "old",
            &ModeEdit { effort: Some(Some(RefineEffort::High)), backend: CLAUDE, ..ModeEdit::default() },
        )
        .unwrap();
        let mode = touched.custom_mode("old").unwrap();
        assert_eq!((mode.model.as_deref(), mode.effort, mode.backend), (Some("opus"), Some(RefineEffort::High), CLAUDE));
    }

    fn pairs(apps: &[(&str, &str)]) -> Vec<(String, String)> {
        apps.iter().map(|(id, name)| (id.to_string(), name.to_string())).collect()
    }

    fn app(id: &str, name: &str) -> ModeApp {
        ModeApp { id: id.into(), name: name.into() }
    }

    #[test]
    fn apps_for_defaults_lists_defaults_then_history_then_running_apps() {
        let modes = set_app_default(&three(), "com.example.notes", "Notes", "formal").unwrap();
        let modes = set_app_default(&modes, "editor.exe", "", BUILTIN_MODE_ID).unwrap();
        let history = pairs(&[("com.example.chat", "Chatter"), ("com.example.Mail", "Mailbox")]);
        let running = pairs(&[("com.example.browser", "Browser"), ("com.example.chat", "Chatter 2")]);
        assert_eq!(
            apps_for_defaults(&modes, &history, &running),
            vec![
                app("com.example.notes", "Notes"),
                // No stored name: the id stands in for it.
                app("editor.exe", "editor.exe"),
                app("com.example.chat", "Chatter"),
                // Ids come back in the stored form; names as given.
                app("com.example.mail", "Mailbox"),
                app("com.example.browser", "Browser"),
            ]
        );
    }

    #[test]
    fn apps_for_defaults_lists_each_app_once_whatever_the_case() {
        let modes = set_app_default(&three(), "Editor.EXE", "", "formal").unwrap();
        let history = pairs(&[("com.example.Mail", "Mail"), ("COM.EXAMPLE.MAIL", "Mail again"), ("  ", "Nameless")]);
        let running = pairs(&[("EDITOR.exe", "Editor"), ("com.example.mail", "Mail now"), ("com.example.new", " ")]);
        assert_eq!(
            apps_for_defaults(&modes, &history, &running),
            vec![
                // The default had no name: the running app supplies it.
                app("editor.exe", "Editor"),
                app("com.example.mail", "Mail"),
                app("com.example.new", "com.example.new"),
            ]
        );
        // With History off and nothing set: the open apps alone.
        assert_eq!(
            apps_for_defaults(&ModesSettings::default(), &[], &pairs(&[("com.example.browser", "Browser")])),
            vec![app("com.example.browser", "Browser")]
        );
        assert!(apps_for_defaults(&ModesSettings::default(), &[], &[]).is_empty());
    }
}
