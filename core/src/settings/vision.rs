//! `vision` block of `settings.json` (TASK-98).
//!
//! Vision lets an ask carry a screenshot of the display the person is
//! working on. This block says *when* that happens ([`VisionTrigger`]),
//! which spoken phrases ask for it, and which apps must never end up
//! in a screenshot.
//!
//! Both lists are `Option`s: `None` means "the built-in list", so a
//! person who never edited a list gets the defaults of whatever
//! version they run, including entries added later. The moment they
//! edit a list it is stored in full and is theirs.
//!
//! The block is read leniently, field by field, and every doubt is
//! settled on the careful side: a trigger this version does not know
//! reads as `off` (never as a mode that captures), and a never-capture
//! list that cannot be read is the built-in one (never an empty one).
//!
//! The types are not behind a cargo feature: a settings file is the
//! same file in every build. The code that takes a screenshot is
//! (`core::vision`, feature `vision`).

use serde::{Deserialize, Deserializer, Serialize};

/// Most phrases kept, and the longest one, in characters.
pub const VISION_PHRASES_MAX: usize = 100;
pub const VISION_PHRASE_MAX_CHARS: usize = 80;
/// Most never-capture apps kept, and the longest id or name.
pub const VISION_NEVER_CAPTURE_MAX: usize = 200;
pub const VISION_APP_FIELD_MAX_CHARS: usize = 200;

/// When an ask includes the screen.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum VisionTrigger {
    /// Never.
    Off,
    /// When the question contains one of the phrases. Matched on the
    /// machine, before anything is sent.
    #[default]
    Phrases,
    /// A phrase, or the model says it cannot answer without the
    /// screen. The question goes out as text first; the screen
    /// follows only when the model asks for it.
    Smart,
    /// Every ask.
    Always,
}

impl VisionTrigger {
    pub const ALL: [VisionTrigger; 4] = [Self::Off, Self::Phrases, Self::Smart, Self::Always];

    /// Stable id, same as the serialized form.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Phrases => "phrases",
            Self::Smart => "smart",
            Self::Always => "always",
        }
    }

    /// The trigger with this id (case and surrounding space ignored).
    pub fn parse(id: &str) -> Option<Self> {
        let id = id.trim().to_lowercase();
        Self::ALL.into_iter().find(|t| t.as_str() == id)
    }
}

/// An app that must never be in a screenshot.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
#[serde(default)]
pub struct NeverCaptureApp {
    /// The id [`crate::focus`] reports for the app, in lower case: the
    /// bundle id on macOS (`com.1password.1password`), the exe file
    /// name on Windows (`1password.exe`). Compared ignoring case.
    pub id: String,
    /// Name to show a person ("1Password"). May be empty.
    pub name: String,
}

impl NeverCaptureApp {
    pub fn new(id: &str, name: &str) -> Self {
        Self { id: id.to_string(), name: name.to_string() }
    }

    /// What to call the app in a sentence: its name, or its id when it
    /// has none.
    pub fn label(&self) -> &str {
        if self.name.trim().is_empty() { &self.id } else { &self.name }
    }
}

/// The `vision` settings block.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
#[serde(default)]
pub struct VisionSettings {
    /// Default: [`VisionTrigger::Phrases`]. A value this version does
    /// not know reads as `off`.
    #[serde(deserialize_with = "lenient_trigger")]
    pub trigger: VisionTrigger,
    /// The phrases that ask for the screen. `None` = the built-in
    /// list ([`default_phrases`]).
    #[serde(deserialize_with = "lenient_phrases")]
    pub phrases: Option<Vec<String>>,
    /// Apps that are never captured. `None` = the built-in list
    /// ([`default_never_capture`]).
    #[serde(deserialize_with = "lenient_never_capture")]
    pub never_capture: Option<Vec<NeverCaptureApp>>,
}

/// A trigger that is not a known id is `off`: a setting we cannot
/// read must not turn into one that captures.
fn lenient_trigger<'de, D: Deserializer<'de>>(d: D) -> Result<VisionTrigger, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(value.as_str().and_then(VisionTrigger::parse).unwrap_or(VisionTrigger::Off))
}

/// Anything but a list is "the built-in list"; entries that are not
/// strings are skipped.
fn lenient_phrases<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<String>>, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(value
        .as_array()
        .map(|items| items.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()))
}

/// Anything but a list is "the built-in list". An entry is an object
/// with `id` (and `name`), or just the id as a string.
fn lenient_never_capture<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<NeverCaptureApp>>, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(value.as_array().map(|items| {
        items
            .iter()
            .filter_map(|item| match item {
                serde_json::Value::String(id) => Some(NeverCaptureApp::new(id, "")),
                serde_json::Value::Object(map) => {
                    let text = |key: &str| map.get(key).and_then(|v| v.as_str()).unwrap_or_default();
                    Some(NeverCaptureApp::new(text("id"), text("name")))
                }
                _ => None,
            })
            .collect()
    }))
}

/// The phrases Vision ships with, English then Danish.
pub fn default_phrases() -> Vec<String> {
    [
        "look at my screen",
        "look at the screen",
        "on my screen",
        "on the screen",
        "what's on my screen",
        "see my screen",
        "this screen",
        "screenshot",
        "screen shot",
        "this message",
        "this email",
        "this error",
        "se på min skærm",
        "kig på min skærm",
        "se på skærmen",
        "kig på skærmen",
        "på min skærm",
        "på skærmen",
        "skærmbillede",
        "skærmbilledet",
        "denne besked",
        "denne mail",
        "denne fejl",
    ]
    .iter()
    .map(|p| (*p).to_string())
    .collect()
}

/// Password managers on macOS, by bundle id (lower case).
const MAC_NEVER_CAPTURE: &[(&str, &str)] = &[
    ("com.1password.1password", "1Password"),
    ("com.agilebits.onepassword7", "1Password 7"),
    ("com.bitwarden.desktop", "Bitwarden"),
    ("org.keepassxc.keepassxc", "KeePassXC"),
    ("com.lastpass.lastpassmacdesktop", "LastPass"),
    ("com.dashlane.dashlanephonefinal", "Dashlane"),
    ("com.dashlane.dashlane", "Dashlane"),
    ("com.apple.passwords", "Passwords"),
    ("com.apple.keychainaccess", "Keychain Access"),
];

/// Password managers on Windows, by exe file name (lower case).
const WINDOWS_NEVER_CAPTURE: &[(&str, &str)] = &[
    ("1password.exe", "1Password"),
    ("bitwarden.exe", "Bitwarden"),
    ("keepassxc.exe", "KeePassXC"),
    ("lastpass.exe", "LastPass"),
    ("dashlane.exe", "Dashlane"),
];

fn apps(list: &[(&str, &str)]) -> Vec<NeverCaptureApp> {
    list.iter().map(|(id, name)| NeverCaptureApp::new(id, name)).collect()
}

/// The never-capture list Vision ships with on this platform: the
/// common password managers. Ids are in lower case, like every id a
/// cleaned list holds; they are compared ignoring case.
pub fn default_never_capture() -> Vec<NeverCaptureApp> {
    if cfg!(target_os = "windows") { apps(WINDOWS_NEVER_CAPTURE) } else { apps(MAC_NEVER_CAPTURE) }
}

/// One line, single spaces, at most `max` characters.
fn tidy(s: &str, max: usize) -> String {
    let line = s.split_whitespace().collect::<Vec<_>>().join(" ");
    line.chars().take(max).collect::<String>().trim_end().to_string()
}

impl VisionSettings {
    /// The phrases in force: the user's list, or the built-in one.
    pub fn effective_phrases(&self) -> Vec<String> {
        self.phrases.clone().unwrap_or_else(default_phrases)
    }

    /// The never-capture list in force: the user's, or the built-in
    /// one.
    pub fn effective_never_capture(&self) -> Vec<NeverCaptureApp> {
        self.never_capture.clone().unwrap_or_else(default_never_capture)
    }

    /// Tidy lists: entries trimmed to one line, empty ones dropped,
    /// doubles dropped (ignoring case), ids in lower case, sizes
    /// capped. A list that is `None` stays `None`; an edited list that
    /// ends up empty stays an empty list (the person removed every
    /// entry, which is theirs to do). Applied on every load and save.
    pub fn cleaned(mut self) -> Self {
        self.phrases = self.phrases.map(|list| {
            let mut out: Vec<String> = Vec::new();
            for phrase in list {
                let phrase = tidy(&phrase, VISION_PHRASE_MAX_CHARS);
                let key = phrase.to_lowercase();
                if phrase.is_empty() || out.iter().any(|p| p.to_lowercase() == key) {
                    continue;
                }
                out.push(phrase);
                if out.len() == VISION_PHRASES_MAX {
                    break;
                }
            }
            out
        });
        self.never_capture = self.never_capture.map(|list| {
            let mut out: Vec<NeverCaptureApp> = Vec::new();
            for app in list {
                let id = tidy(&app.id, VISION_APP_FIELD_MAX_CHARS).to_lowercase();
                if id.is_empty() || out.iter().any(|a| a.id == id) {
                    continue;
                }
                out.push(NeverCaptureApp { id, name: tidy(&app.name, VISION_APP_FIELD_MAX_CHARS) });
                if out.len() == VISION_NEVER_CAPTURE_MAX {
                    break;
                }
            }
            out
        });
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_phrases_with_the_built_in_lists() {
        let s = VisionSettings::default();
        assert_eq!(s.trigger, VisionTrigger::Phrases);
        assert_eq!((s.phrases.clone(), s.never_capture.clone()), (None, None));
        assert_eq!(s.clone().cleaned(), s);
        assert_eq!(s.effective_phrases(), default_phrases());
        assert_eq!(s.effective_never_capture(), default_never_capture());
        // Nothing of the defaults is written out: a later version's
        // defaults reach a person who never edited a list.
        assert_eq!(
            serde_json::to_string(&s).unwrap(),
            r#"{"trigger":"phrases","phrases":null,"never_capture":null}"#
        );
    }

    #[test]
    fn the_built_in_lists_are_clean_and_cover_both_languages_and_platforms() {
        let phrases = default_phrases();
        for must in ["on my screen", "what's on my screen", "this error", "på min skærm", "skærmbillede", "denne fejl"] {
            assert!(phrases.iter().any(|p| p == must), "missing {must}");
        }
        // The defaults are what cleaning would leave of them.
        let cleaned = VisionSettings {
            phrases: Some(phrases.clone()),
            never_capture: Some(apps(MAC_NEVER_CAPTURE).into_iter().chain(apps(WINDOWS_NEVER_CAPTURE)).collect()),
            ..VisionSettings::default()
        }
        .cleaned();
        assert_eq!(cleaned.phrases.unwrap(), phrases);
        assert_eq!(cleaned.never_capture.unwrap().len(), MAC_NEVER_CAPTURE.len() + WINDOWS_NEVER_CAPTURE.len());

        let mac: Vec<&str> = MAC_NEVER_CAPTURE.iter().map(|(id, _)| *id).collect();
        for id in [
            "com.1password.1password",
            "com.agilebits.onepassword7",
            "com.bitwarden.desktop",
            "org.keepassxc.keepassxc",
            "com.lastpass.lastpassmacdesktop",
            "com.dashlane.dashlanephonefinal",
            "com.dashlane.dashlane",
            "com.apple.passwords",
            "com.apple.keychainaccess",
        ] {
            assert!(mac.contains(&id), "missing {id}");
        }
        let windows: Vec<&str> = WINDOWS_NEVER_CAPTURE.iter().map(|(id, _)| *id).collect();
        assert_eq!(windows, ["1password.exe", "bitwarden.exe", "keepassxc.exe", "lastpass.exe", "dashlane.exe"]);
        assert!(!default_never_capture().is_empty());
        assert!(default_never_capture().iter().all(|a| !a.name.is_empty() && a.id == a.id.to_lowercase()));
    }

    #[test]
    fn triggers_have_stable_ids() {
        for t in VisionTrigger::ALL {
            assert_eq!(serde_json::to_string(&t).unwrap(), format!("\"{}\"", t.as_str()));
            assert_eq!(VisionTrigger::parse(t.as_str()), Some(t));
        }
        assert_eq!(VisionTrigger::parse(" Smart "), Some(VisionTrigger::Smart));
        assert_eq!(VisionTrigger::parse("sometimes"), None);
    }

    #[test]
    fn missing_fields_fill_defaults_and_doubt_is_settled_on_the_careful_side() {
        let s: VisionSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(s, VisionSettings::default());
        let s: VisionSettings = serde_json::from_str(r#"{"trigger":"always"}"#).unwrap();
        assert_eq!(s.trigger, VisionTrigger::Always);
        assert_eq!(s.phrases, None);

        // A trigger this version does not know never captures.
        for raw in [r#"{"trigger":"continuous"}"#, r#"{"trigger":7}"#, r#"{"trigger":null}"#] {
            let s: VisionSettings = serde_json::from_str(raw).unwrap();
            assert_eq!(s.trigger, VisionTrigger::Off, "{raw}");
        }
        // A broken list is the built-in list and does not cost the
        // block: the trigger the person chose still holds.
        let s: VisionSettings =
            serde_json::from_str(r#"{"trigger":"off","phrases":"on my screen","never_capture":{"id":"x"}}"#).unwrap();
        assert_eq!(s.trigger, VisionTrigger::Off);
        assert_eq!((s.phrases.clone(), s.never_capture.clone()), (None, None));
        assert_eq!(s.effective_never_capture(), default_never_capture());

        // Entries of the wrong shape are skipped; an id alone is fine.
        let s: VisionSettings = serde_json::from_str(
            r#"{"phrases":["on my screen",5,null],"never_capture":["Bank.App",{"id":"com.example.vault","name":"Vault"},4,{"name":"no id"}]}"#,
        )
        .unwrap();
        let s = s.cleaned();
        assert_eq!(s.phrases.unwrap(), ["on my screen"]);
        assert_eq!(
            s.never_capture.unwrap(),
            [NeverCaptureApp::new("bank.app", ""), NeverCaptureApp::new("com.example.vault", "Vault")]
        );
    }

    #[test]
    fn cleaned_trims_dedupes_lowercases_ids_and_caps() {
        let s = VisionSettings {
            trigger: VisionTrigger::Smart,
            phrases: Some(vec![
                "  On my   screen ".into(),
                "".into(),
                "on MY screen".into(),
                "på skærmen".into(),
                "x".repeat(VISION_PHRASE_MAX_CHARS + 20),
            ]),
            never_capture: Some(vec![
                NeverCaptureApp::new("  Com.Example.Vault ", "  Vault  "),
                NeverCaptureApp::new("com.example.vault", "Again"),
                NeverCaptureApp::new("   ", "No id"),
                NeverCaptureApp::new("Brennholt.EXE", ""),
            ]),
        }
        .cleaned();
        assert_eq!(s.trigger, VisionTrigger::Smart);
        let phrases = s.phrases.clone().unwrap();
        assert_eq!(phrases[..2], ["On my screen", "på skærmen"]);
        assert_eq!(phrases[2].chars().count(), VISION_PHRASE_MAX_CHARS);
        assert_eq!(
            s.never_capture.clone().unwrap(),
            [NeverCaptureApp::new("com.example.vault", "Vault"), NeverCaptureApp::new("brennholt.exe", "")]
        );
        assert_eq!(s.never_capture.as_ref().unwrap()[1].label(), "brennholt.exe");
        assert_eq!(s.never_capture.as_ref().unwrap()[0].label(), "Vault");
        assert_eq!(s.clone().cleaned(), s);

        // An edited list that is empty stays empty: it is not the
        // built-in list coming back.
        let emptied = VisionSettings { phrases: Some(vec![" ".into()]), never_capture: Some(vec![]), ..s }.cleaned();
        assert_eq!(emptied.effective_phrases(), Vec::<String>::new());
        assert_eq!(emptied.effective_never_capture(), Vec::<NeverCaptureApp>::new());

        let many = VisionSettings {
            phrases: Some((0..VISION_PHRASES_MAX + 30).map(|i| format!("phrase {i}")).collect()),
            never_capture: Some(
                (0..VISION_NEVER_CAPTURE_MAX + 30).map(|i| NeverCaptureApp::new(&format!("app{i}.exe"), "")).collect(),
            ),
            ..VisionSettings::default()
        }
        .cleaned();
        assert_eq!(many.phrases.unwrap().len(), VISION_PHRASES_MAX);
        assert_eq!(many.never_capture.unwrap().len(), VISION_NEVER_CAPTURE_MAX);
    }
}
