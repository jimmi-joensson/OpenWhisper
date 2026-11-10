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
//!
//! What a mode does to the prompt and the model call is in
//! `crate::refine` (feature `refine`).

use serde::{Deserialize, Serialize};

use crate::settings::{
    AppDefault, BUILTIN_MODE_ID, BUILTIN_MODE_NAME, MODES_MAX, Mode, ModesSettings, RefineEffort,
    RefineSettings,
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
            builtin: false,
        }
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
        let default = modes.app_defaults.iter().find(|d| !app_id.is_empty() && d.app_id == app_id);
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
}

fn unknown(id: &str) -> String {
    format!("There is no mode with the id \"{id}\".")
}

/// Add a custom mode at the end of the list. Its id is made from the
/// name; read it from the last entry of `custom` in the result.
pub fn add(
    modes: &ModesSettings,
    name: &str,
    instructions: &str,
    model: Option<&str>,
    effort: Option<RefineEffort>,
) -> Result<ModesSettings, String> {
    if name.trim().is_empty() {
        return Err("A mode needs a name.".to_string());
    }
    if modes.custom.len() >= MODES_MAX {
        return Err(format!("There are already {MODES_MAX} modes. Delete one before adding another."));
    }
    let mut next = modes.clone();
    next.custom.push(Mode {
        id: String::new(),
        name: name.to_string(),
        instructions: instructions.to_string(),
        model: model.map(str::to_string),
        effort,
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
        mode.name = name.clone();
    }
    if let Some(instructions) = &changes.instructions {
        mode.instructions = instructions.clone();
    }
    if let Some(model) = &changes.model {
        mode.model = model.clone();
    }
    if let Some(effort) = changes.effort {
        mode.effort = effort;
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
    let known_name = next
        .app_defaults
        .iter()
        .find(|d| d.app_id == app_id)
        .map(|d| d.app_name.clone())
        .unwrap_or_default();
    let app_name = if app_name.trim().is_empty() { known_name } else { app_name.trim().to_string() };
    next.app_defaults.push(AppDefault {
        app_id: app_id.to_string(),
        app_name,
        mode: mode_id.to_string(),
    });
    Ok(next.cleaned())
}

/// Remove an app's default mode, so it follows the selected mode
/// again.
pub fn clear_app_default(modes: &ModesSettings, app_id: &str) -> Result<ModesSettings, String> {
    let app_id = app_id.trim();
    if !modes.app_defaults.iter().any(|d| d.app_id == app_id) {
        return Err(format!("The app \"{app_id}\" has no default mode."));
    }
    let mut next = modes.clone();
    next.app_defaults.retain(|d| d.app_id != app_id);
    Ok(next.cleaned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refine() -> RefineSettings {
        RefineSettings { instructions: "Use British spelling.".into(), ..RefineSettings::default() }
    }

    /// Dictation plus "Formal" and "Write in English".
    fn three() -> ModesSettings {
        let m = add(&ModesSettings::default(), "Formal", "Rewrite it in a formal tone.", None, None).unwrap();
        add(&m, "Write in English", "Write it in English.", Some("opus"), Some(RefineEffort::Medium)).unwrap()
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
        let modes = add(&three(), "Careful", "  ", Some("opus"), None).unwrap();
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
        let again = add(&modes, "Formal", "Another one.", None, None).unwrap();
        assert_eq!(again.custom.last().unwrap().id, "formal-2");
        // Selecting nothing new: adding does not change the selection.
        assert_eq!(again.active, BUILTIN_MODE_ID);
        assert_eq!(add(&modes, "   ", "x", None, None).unwrap_err(), "A mode needs a name.");

        let mut full = ModesSettings::default();
        for i in 0..MODES_MAX {
            full = add(&full, &format!("Mode {i}"), "x", None, None).unwrap();
        }
        let err = add(&full, "One too many", "x", None, None).unwrap_err();
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
}
