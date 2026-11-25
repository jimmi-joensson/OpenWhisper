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

use super::{RefineBackendKind, RefineEffort};

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
    /// The Refine backend `model` and `effort` were chosen for. A
    /// model name means something to one backend only ("opus" is not
    /// a model of an OpenAI-compatible server), so they apply while
    /// that backend is the one in use and the mode follows the Refine
    /// settings otherwise. `None` in a block written before this
    /// field existed: such a model or effort is for the Claude CLI
    /// (see [`Mode::override_backend`]). Always `None` for a mode
    /// without a model or effort of its own.
    #[serde(deserialize_with = "lenient_backend")]
    pub backend: Option<RefineBackendKind>,
}

impl Mode {
    /// The backend this mode's own model and effort are for, or
    /// `None` when it has neither.
    pub fn override_backend(&self) -> Option<RefineBackendKind> {
        (self.model.is_some() || self.effort.is_some())
            .then(|| self.backend.unwrap_or(RefineBackendKind::ClaudeCli))
    }
}

/// "In this app, use that mode."
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(default)]
pub struct AppDefault {
    /// See [`crate::focus::FocusContext::app_id`]: the bundle id on
    /// macOS, the exe file name on Windows. Stored in lower case
    /// ([`app_key`]): two spellings of one id are one app.
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

pub(super) fn lenient_effort<'de, D: Deserializer<'de>>(d: D) -> Result<Option<RefineEffort>, D::Error> {
    let raw: Option<serde_json::Value> = Option::deserialize(d)?;
    Ok(raw
        .as_ref()
        .and_then(serde_json::Value::as_str)
        .and_then(RefineEffort::parse))
}

fn lenient_backend<'de, D: Deserializer<'de>>(d: D) -> Result<Option<RefineBackendKind>, D::Error> {
    let raw: Option<serde_json::Value> = Option::deserialize(d)?;
    Ok(raw
        .as_ref()
        .and_then(serde_json::Value::as_str)
        .and_then(RefineBackendKind::parse))
}

/// The form an app id is stored and compared in: trimmed, lower case.
/// Bundle ids and exe names do not differ by case alone, but they are
/// typed by hand (`openwhisper modes app set`) and reported with
/// whatever case the app ships, so `com.example.Mail` and
/// `com.example.mail` are the same app.
pub fn app_key(app_id: &str) -> String {
    app_id.trim().to_lowercase()
}

/// The form a mode name is compared in: trimmed, lower case. Two
/// modes whose names differ only by case or outer space would be one
/// name twice in a menu.
pub fn mode_name_key(name: &str) -> String {
    name.trim().to_lowercase()
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
    /// - Names are unique, ignoring case ([`mode_name_key`]), and
    ///   never the built-in mode's name. The first mode keeps its
    ///   name; a later one with the same name gets a number after it
    ///   ("Formal 2", "Formal 3", ...). [`crate::modes::add`] and
    ///   [`crate::modes::edit`] refuse such a name outright, so this
    ///   only ever happens to a file edited by hand.
    /// - Every id is a slug ([`mode_slug`]), made from the name when
    ///   the mode has none. Ids are unique (`formal`, `formal-2`, ...)
    ///   and never [`BUILTIN_MODE_ID`].
    /// - An empty model is no model. A mode without a model or effort
    ///   has no backend either.
    /// - A selection that names no mode falls back to Dictation.
    /// - App ids are stored in lower case ([`app_key`]). An app
    ///   default without an app id, or for a mode that does not exist,
    ///   is dropped. One default per app; the last wins.
    pub fn cleaned(self) -> Self {
        let mut custom: Vec<Mode> = Vec::new();
        for mode in self.custom {
            let given = capped(&mode.name, MODE_NAME_MAX_CHARS);
            if given.is_empty() {
                continue;
            }
            if custom.len() >= MODES_MAX {
                break;
            }
            let mut base = mode_slug(&mode.id);
            if base.is_empty() {
                base = mode_slug(&given);
            }
            let name_taken = |name: &str| {
                let key = mode_name_key(name);
                key == mode_name_key(BUILTIN_MODE_NAME) || custom.iter().any(|m| mode_name_key(&m.name) == key)
            };
            let mut name = given.clone();
            let mut n = 2;
            while name_taken(&name) {
                // Room for the number is taken from the end of the name.
                let suffix = format!(" {n}");
                let keep = MODE_NAME_MAX_CHARS.saturating_sub(suffix.chars().count());
                name = format!("{}{suffix}", capped(&given, keep));
                n += 1;
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
            let model = mode.model.map(|m| m.trim().to_string()).filter(|m| !m.is_empty());
            let backend = if model.is_some() || mode.effort.is_some() { mode.backend } else { None };
            custom.push(Mode {
                id,
                name,
                instructions: capped(&mode.instructions, MODE_INSTRUCTIONS_MAX_CHARS),
                model,
                effort: mode.effort,
                backend,
            });
        }

        let exists = |id: &str| id == BUILTIN_MODE_ID || custom.iter().any(|m| m.id == id);

        let active = mode_slug(&self.active);
        let active = if exists(&active) { active } else { BUILTIN_MODE_ID.to_string() };

        let mut app_defaults: Vec<AppDefault> = Vec::new();
        for default in self.app_defaults {
            let app_id = app_key(&default.app_id);
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
    fn names_edited_into_duplicates_get_a_number() {
        let s = ModesSettings {
            custom: vec![
                mode("", "Formal"),
                mode("", " formal "),
                mode("", "FORMAL"),
                // The built-in mode's name is taken before any of them.
                mode("", "dictation"),
                // A name that is already numbered keeps its number.
                mode("", "Formal 2"),
                // The number has to fit inside the longest name.
                mode("", &"n".repeat(MODE_NAME_MAX_CHARS)),
                mode("", &"N".repeat(MODE_NAME_MAX_CHARS)),
            ],
            ..ModesSettings::default()
        }
        .cleaned();
        let names: Vec<&str> = s.custom.iter().map(|m| m.name.as_str()).collect();
        let long = "n".repeat(MODE_NAME_MAX_CHARS);
        let numbered = format!("{} 2", "N".repeat(MODE_NAME_MAX_CHARS - 2));
        assert_eq!(
            names,
            ["Formal", "formal 2", "FORMAL 3", "dictation 2", "Formal 2 2", long.as_str(), numbered.as_str()]
        );
        assert!(s.custom.iter().all(|m| m.name.chars().count() <= MODE_NAME_MAX_CHARS));
        let mut keys: Vec<String> = s.custom.iter().map(|m| mode_name_key(&m.name)).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), s.custom.len(), "every name is distinct, ignoring case");
        // Cleaning again changes nothing.
        assert_eq!(s.clone().cleaned(), s);
    }

    #[test]
    fn app_ids_are_stored_in_lower_case_and_one_app_has_one_default() {
        let s = ModesSettings {
            custom: vec![mode("", "Formal")],
            app_defaults: vec![
                AppDefault { app_id: "com.example.Mail".into(), app_name: "Mail".into(), mode: "formal".into() },
                AppDefault { app_id: " COM.EXAMPLE.MAIL ".into(), app_name: "Mail Pro".into(), mode: "dictation".into() },
                default_for("Editor.EXE", "formal"),
            ],
            ..ModesSettings::default()
        }
        .cleaned();
        assert_eq!(
            s.app_defaults,
            vec![
                // The name keeps the case it was given.
                AppDefault { app_id: "com.example.mail".into(), app_name: "Mail Pro".into(), mode: "dictation".into() },
                default_for("editor.exe", "formal"),
            ]
        );
        assert_eq!(app_key("  Slack.EXE "), "slack.exe");
    }

    #[test]
    fn a_backend_is_kept_only_with_a_model_or_an_effort() {
        let with = |model: Option<&str>, effort, backend| Mode {
            name: format!("M {model:?} {effort:?}"),
            model: model.map(str::to_string),
            effort,
            backend,
            ..Mode::default()
        };
        let openai = Some(RefineBackendKind::OpenaiCompatible);
        let s = ModesSettings {
            custom: vec![
                with(Some("small"), None, openai),
                with(None, Some(RefineEffort::High), openai),
                with(None, None, openai),
                with(Some("  "), None, openai),
                // Written before the field existed.
                with(Some("opus"), None, None),
            ],
            ..ModesSettings::default()
        }
        .cleaned();
        let backends: Vec<_> = s.custom.iter().map(|m| m.backend).collect();
        assert_eq!(backends, [openai, openai, None, None, None]);
        let for_backend: Vec<_> = s.custom.iter().map(Mode::override_backend).collect();
        assert_eq!(for_backend, [openai, openai, None, None, Some(RefineBackendKind::ClaudeCli)]);

        // A backend this app does not know reads as none, and an old
        // block has none.
        let odd: ModesSettings = serde_json::from_str(
            r#"{"custom":[{"name":"A","model":"x","backend":"telepathy"},{"name":"B","model":"opus"}]}"#,
        )
        .unwrap();
        assert_eq!((odd.custom[0].backend, odd.custom[1].backend), (None, None));
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
                backend: Some(RefineBackendKind::ClaudeCli),
            }],
            app_defaults: vec![AppDefault {
                app_id: "com.example.mail".into(),
                app_name: "Mail".into(),
                mode: "formal".into(),
            }],
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"effort\":\"high\""), "{json}");
        assert!(json.contains("\"backend\":\"claude-cli\""), "{json}");
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
