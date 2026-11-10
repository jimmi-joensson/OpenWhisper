//! `modes` block of `settings.json` (TASK-96).
//!
//! A mode is a name plus instructions for the Refine step, optionally
//! with its own model and effort. There is one built-in mode,
//! "Dictation": today's cleanup, whose instructions are the Refine
//! instructions ([`super::RefineSettings::instructions`]). It is not
//! stored here, so it cannot be renamed or lost. Every other mode is
//! made by the user and lives in [`ModesSettings::custom`].
//!
//! The block also holds which mode is selected and which apps have a
//! default mode of their own. What runs for a dictation is worked out
//! in [`crate::modes::resolve`]; this file is only the stored shape
//! and the tidying every load and save applies to it.

use serde::{Deserialize, Deserializer, Serialize};

use super::RefineEffort;

/// Id of the built-in mode. Never stored in [`ModesSettings::custom`].
pub const BUILTIN_MODE_ID: &str = "dictation";
/// Name of the built-in mode, as shown to a person.
pub const BUILTIN_MODE_NAME: &str = "Dictation";

/// Longest mode name kept, in characters. A name has to fit a menu
/// row and the tray.
pub const MODE_NAME_MAX_CHARS: usize = 40;
/// Longest mode instructions kept, in characters. They ride along in
/// the prompt of every dictation in that mode.
pub const MODE_INSTRUCTIONS_MAX_CHARS: usize = 4000;
/// Most custom modes kept.
pub const MODES_MAX: usize = 50;

/// One mode the user made.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(default)]
pub struct Mode {
    /// Stable id: a slug made from the first name the mode had. Stays
    /// the same when the mode is renamed, so the selection and the
    /// app defaults keep pointing at it.
    pub id: String,
    pub name: String,
    /// What the model is asked to do with the dictated text ("Rewrite
    /// what I say as a prompt for a coding agent"). Left empty, the
    /// mode cleans up exactly like Dictation and differs only in its
    /// model or effort.
    pub instructions: String,
    /// Model for this mode. `None` = the model in the Refine settings.
    pub model: Option<String>,
    /// Effort for this mode. `None` = the effort in the Refine
    /// settings. A value this app does not know reads as `None`
    /// instead of costing the user the whole block.
    #[serde(deserialize_with = "lenient_effort")]
    pub effort: Option<RefineEffort>,
}

/// "In this app, use that mode."
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(default)]
pub struct AppDefault {
    /// See [`crate::focus::FocusContext::app_id`]: the bundle id on
    /// macOS, the exe file name in lower case on Windows.
    pub app_id: String,
    /// Name to show a person. May be empty; then the id is shown.
    pub app_name: String,
    /// Id of the mode: a custom mode's id or [`BUILTIN_MODE_ID`].
    pub mode: String,
}

/// The `modes` settings block.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(default)]
pub struct ModesSettings {
    /// Id of the mode the user selected. [`BUILTIN_MODE_ID`] until
    /// they pick another.
    pub active: String,
    /// The user's own modes, in the order they are listed and cycled.
    pub custom: Vec<Mode>,
    /// At most one default mode per app.
    pub app_defaults: Vec<AppDefault>,
}

impl Default for ModesSettings {
    fn default() -> Self {
        Self {
            active: BUILTIN_MODE_ID.to_string(),
            custom: Vec::new(),
            app_defaults: Vec::new(),
        }
    }
}

fn lenient_effort<'de, D: Deserializer<'de>>(d: D) -> Result<Option<RefineEffort>, D::Error> {
    let raw: Option<serde_json::Value> = Option::deserialize(d)?;
    Ok(raw
        .as_ref()
        .and_then(serde_json::Value::as_str)
        .and_then(RefineEffort::parse))
}

/// Turn a name (or a hand-written id) into an id: lower case, letters
/// and digits kept, every run of anything else one hyphen.
/// `"Prompt for a coding agent"` becomes `prompt-for-a-coding-agent`.
/// Empty when nothing usable is left.
pub fn mode_slug(text: &str) -> String {
    let mut slug = String::new();
    for c in text.trim().chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            slug.push(c);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_end_matches('-').to_string()
}

fn capped(text: &str, max_chars: usize) -> String {
    text.trim().chars().take(max_chars).collect::<String>().trim_end().to_string()
}

impl ModesSettings {
    /// The custom mode with this id.
    pub fn custom_mode(&self, id: &str) -> Option<&Mode> {
        self.custom.iter().find(|m| m.id == id)
    }

    /// `id` names a mode that exists: the built-in one or a custom one.
    pub fn has_mode(&self, id: &str) -> bool {
        id == BUILTIN_MODE_ID || self.custom_mode(id).is_some()
    }

    /// A block that holds together, whatever was in the file. Applied
    /// on every load and save, so a hand-edited file cannot leave the
    /// app with two modes under one id, a selection that points
    /// nowhere or a default for a mode that is gone.
    ///
    /// - Names and instructions are trimmed and cut to
    ///   [`MODE_NAME_MAX_CHARS`] and [`MODE_INSTRUCTIONS_MAX_CHARS`];
    ///   a mode without a name is dropped; at most [`MODES_MAX`] stay.
    /// - Every id is a slug ([`mode_slug`]), made from the name when
    ///   the mode has none. Ids are unique (`formal`, `formal-2`, ...)
    ///   and never [`BUILTIN_MODE_ID`].
    /// - An empty model is no model.
    /// - A selection that names no mode falls back to Dictation.
    /// - An app default without an app id, or for a mode that does
    ///   not exist, is dropped. One default per app; the last wins.
    pub fn cleaned(self) -> Self {
        let mut custom: Vec<Mode> = Vec::new();
        for mode in self.custom {
            let name = capped(&mode.name, MODE_NAME_MAX_CHARS);
            if name.is_empty() {
                continue;
            }
            if custom.len() >= MODES_MAX {
                break;
            }
            let mut base = mode_slug(&mode.id);
            if base.is_empty() {
                base = mode_slug(&name);
            }
            if base.is_empty() {
                base = "mode".to_string();
            }
            let taken = |id: &str| id == BUILTIN_MODE_ID || custom.iter().any(|m| m.id == id);
            let mut id = base.clone();
            let mut n = 2;
            while taken(&id) {
                id = format!("{base}-{n}");
                n += 1;
            }
            custom.push(Mode {
                id,
                name,
                instructions: capped(&mode.instructions, MODE_INSTRUCTIONS_MAX_CHARS),
                model: mode.model.map(|m| m.trim().to_string()).filter(|m| !m.is_empty()),
                effort: mode.effort,
            });
        }

        let exists = |id: &str| id == BUILTIN_MODE_ID || custom.iter().any(|m| m.id == id);

        let active = mode_slug(&self.active);
        let active = if exists(&active) { active } else { BUILTIN_MODE_ID.to_string() };

        let mut app_defaults: Vec<AppDefault> = Vec::new();
        for default in self.app_defaults {
            let app_id = default.app_id.trim().to_string();
            let mode = mode_slug(&default.mode);
            if app_id.is_empty() || !exists(&mode) {
                continue;
            }
            let entry = AppDefault { app_id, app_name: default.app_name.trim().to_string(), mode };
            match app_defaults.iter_mut().find(|d| d.app_id == entry.app_id) {
                Some(slot) => *slot = entry,
                None => app_defaults.push(entry),
            }
        }

        Self { active, custom, app_defaults }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mode(id: &str, name: &str) -> Mode {
        Mode { id: id.into(), name: name.into(), instructions: "Rewrite it.".into(), ..Mode::default() }
    }

    fn default_for(app: &str, mode: &str) -> AppDefault {
        AppDefault { app_id: app.into(), app_name: String::new(), mode: mode.into() }
    }

    #[test]
    fn default_is_the_built_in_mode_and_nothing_else() {
        let s = ModesSettings::default();
        assert_eq!(s.active, BUILTIN_MODE_ID);
        assert!(s.custom.is_empty());
        assert!(s.app_defaults.is_empty());
        assert_eq!(s.clone().cleaned(), s);
        // An empty block in the file is the same thing.
        let parsed: ModesSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(parsed, s);
    }

    #[test]
    fn slugs_are_lower_case_words_joined_by_hyphens() {
        assert_eq!(mode_slug("Prompt for a coding agent"), "prompt-for-a-coding-agent");
        assert_eq!(mode_slug("  Formal!  "), "formal");
        assert_eq!(mode_slug("E-mail (kort)"), "e-mail-kort");
        assert_eq!(mode_slug("Skriv på engelsk"), "skriv-på-engelsk");
        assert_eq!(mode_slug("--- ???"), "");
    }

    #[test]
    fn ids_come_from_the_name_and_stay_unique() {
        let s = ModesSettings {
            custom: vec![mode("", "Formal"), mode("", "formal"), mode("", "Formal!"), mode("", "???")],
            ..ModesSettings::default()
        }
        .cleaned();
        let ids: Vec<&str> = s.custom.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["formal", "formal-2", "formal-3", "mode"]);
        // Cleaning again changes nothing.
        assert_eq!(s.clone().cleaned(), s);
    }

    #[test]
    fn a_custom_mode_never_takes_the_built_in_id() {
        let s = ModesSettings {
            active: "dictation".into(),
            custom: vec![mode("dictation", "My dictation"), mode("", "Dictation")],
            ..ModesSettings::default()
        }
        .cleaned();
        let ids: Vec<&str> = s.custom.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["dictation-2", "dictation-3"]);
        assert_eq!(s.active, BUILTIN_MODE_ID);
        assert!(s.custom_mode(BUILTIN_MODE_ID).is_none());
        assert!(s.has_mode(BUILTIN_MODE_ID));
    }

    #[test]
    fn an_id_survives_a_rename() {
        let s = ModesSettings { custom: vec![mode("formal", "Business tone")], ..ModesSettings::default() }.cleaned();
        assert_eq!(s.custom[0].id, "formal");
        assert_eq!(s.custom[0].name, "Business tone");
    }

    #[test]
    fn names_and_instructions_are_trimmed_and_capped_and_nameless_modes_go() {
        let s = ModesSettings {
            custom: vec![
                Mode {
                    name: format!("  {}  ", "n".repeat(60)),
                    instructions: format!(" {} ", "i".repeat(5000)),
                    model: Some("   ".into()),
                    ..Mode::default()
                },
                mode("ghost", "   "),
                Mode { name: "English".into(), model: Some(" opus ".into()), ..Mode::default() },
            ],
            ..ModesSettings::default()
        }
        .cleaned();
        assert_eq!(s.custom.len(), 2);
        assert_eq!(s.custom[0].name.chars().count(), MODE_NAME_MAX_CHARS);
        assert_eq!(s.custom[0].instructions.chars().count(), MODE_INSTRUCTIONS_MAX_CHARS);
        assert_eq!(s.custom[0].model, None);
        assert_eq!(s.custom[1].model.as_deref(), Some("opus"));
        assert_eq!(s.custom[1].instructions, "");
    }

    #[test]
    fn at_most_fifty_modes_are_kept() {
        let s = ModesSettings {
            custom: (0..70).map(|i| mode("", &format!("Mode {i}"))).collect(),
            ..ModesSettings::default()
        }
        .cleaned();
        assert_eq!(s.custom.len(), MODES_MAX);
        assert_eq!(s.custom[49].name, "Mode 49");
    }

    #[test]
    fn a_selection_that_points_nowhere_falls_back_to_dictation() {
        let s = ModesSettings { active: "gone".into(), custom: vec![mode("", "Formal")], ..ModesSettings::default() }
            .cleaned();
        assert_eq!(s.active, BUILTIN_MODE_ID);
        let s = ModesSettings { active: " Formal ".into(), custom: vec![mode("", "Formal")], ..ModesSettings::default() }
            .cleaned();
        assert_eq!(s.active, "formal");
        let s = ModesSettings { active: String::new(), ..ModesSettings::default() }.cleaned();
        assert_eq!(s.active, BUILTIN_MODE_ID);
    }

    #[test]
    fn app_defaults_need_an_app_and_a_mode_that_exists() {
        let s = ModesSettings {
            custom: vec![mode("", "Formal"), mode("", "English")],
            app_defaults: vec![
                default_for("com.example.mail", "formal"),
                default_for("  ", "formal"),
                default_for("com.example.chat", "gone"),
                // The built-in mode can be an app's default too.
                default_for("com.example.terminal", "dictation"),
                // One per app: the last one wins, in the first one's place.
                AppDefault { app_id: " com.example.mail ".into(), app_name: " Mail ".into(), mode: "english".into() },
            ],
            ..ModesSettings::default()
        }
        .cleaned();
        assert_eq!(
            s.app_defaults,
            vec![
                AppDefault { app_id: "com.example.mail".into(), app_name: "Mail".into(), mode: "english".into() },
                default_for("com.example.terminal", "dictation"),
            ]
        );
    }

    #[test]
    fn the_block_round_trips_and_an_unknown_effort_does_not_cost_the_modes() {
        let s = ModesSettings {
            active: "formal".into(),
            custom: vec![Mode {
                id: "formal".into(),
                name: "Formal".into(),
                instructions: "Rewrite it in a formal tone.".into(),
                model: Some("opus".into()),
                effort: Some(RefineEffort::High),
            }],
            app_defaults: vec![AppDefault {
                app_id: "com.example.mail".into(),
                app_name: "Mail".into(),
                mode: "formal".into(),
            }],
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"effort\":\"high\""), "{json}");
        assert_eq!(serde_json::from_str::<ModesSettings>(&json).unwrap(), s);

        let odd: ModesSettings = serde_json::from_str(
            r#"{"active":"formal","custom":[{"id":"formal","name":"Formal","effort":"turbo"}]}"#,
        )
        .unwrap();
        assert_eq!(odd.custom.len(), 1);
        assert_eq!(odd.custom[0].effort, None);
        assert_eq!(odd.cleaned().active, "formal");
    }
}
