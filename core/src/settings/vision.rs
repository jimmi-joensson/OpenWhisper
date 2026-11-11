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
//! list that cannot be read in full never protects fewer apps than the
//! built-in one. `null` or something that is not a list is the
//! built-in list. A list with even one entry that cannot be read (a
//! number, an object without an `id`) is the built-in list plus every
//! entry that could be read: a typo in a hand-edited file must not
//! take the password managers off the list. Only a well-formed empty
//! list, `[]`, protects nothing: that is the person's own choice, and
//! the settings say so in plain words while it is in force.
//!
//! The trigger a person never chose depends on the platform
//! ([`default_trigger_on`]): `phrases` on macOS, `off` on Windows,
//! where capture has not been verified on a real machine yet and
//! painting is the only layer that keeps a listed app out of an image.
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
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum VisionTrigger {
    /// Never.
    Off,
    /// When the question contains one of the phrases. Matched on the
    /// machine, before anything is sent.
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

/// The trigger of a person who never chose one.
///
/// - macOS: [`VisionTrigger::Phrases`].
/// - Windows: [`VisionTrigger::Off`]. Screen capture there has not
///   been verified on a real machine yet, and nothing can be left out
///   of a capture: a listed app's windows are painted over, which is
///   the only layer. Until that has been checked, Vision is something
///   a person switches on, not something they find switched on.
///
/// A pure function of the platform so both answers are tested on
/// every machine.
pub fn default_trigger_on(windows: bool) -> VisionTrigger {
    if windows { VisionTrigger::Off } else { VisionTrigger::Phrases }
}

/// [`default_trigger_on`] for the platform of this build.
pub fn default_trigger() -> VisionTrigger {
    default_trigger_on(cfg!(target_os = "windows"))
}

impl Default for VisionTrigger {
    fn default() -> Self {
        default_trigger()
    }
}

/// The `vision` settings block.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
#[serde(default)]
pub struct VisionSettings {
    /// Default: [`default_trigger`] (`phrases` on macOS, `off` on
    /// Windows). A value this version does not know reads as `off`.
    #[serde(deserialize_with = "lenient_trigger")]
    pub trigger: VisionTrigger,
    /// The phrases that ask for the screen. `None` = the built-in
    /// list ([`default_phrases`]).
    #[serde(deserialize_with = "lenient_phrases")]
    pub phrases: Option<Vec<String>>,
    /// Apps that are never captured. `None` = the built-in list
    /// ([`default_never_capture`]). `Some` of an empty list protects
    /// nothing; only a well-formed `[]` reads as that.
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

/// One entry of a never-capture list as it stands in the file: an
/// object with a non-empty `id` (and maybe a `name`), or just the id
/// as a string. `None` for anything else.
fn read_app(item: &serde_json::Value) -> Option<NeverCaptureApp> {
    let app = match item {
        serde_json::Value::String(id) => NeverCaptureApp::new(id, ""),
        serde_json::Value::Object(map) => {
            let id = map.get("id")?.as_str()?;
            // A name of the wrong type costs the name, not the entry.
            let name = map.get("name").and_then(|v| v.as_str()).unwrap_or_default();
            NeverCaptureApp::new(id, name)
        }
        _ => return None,
    };
    (!app.id.trim().is_empty()).then_some(app)
}

/// The never-capture list of a settings file, read so that a mistake
/// in it can only add protection (see the module docs):
///
/// - `null`, or anything that is not a list: the built-in list;
/// - a list whose entries can all be read: that list, also when it is
///   empty;
/// - a list with an entry that cannot be read: the built-in list, and
///   after it every entry that could be read.
fn lenient_never_capture<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<NeverCaptureApp>>, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    let Some(items) = value.as_array() else {
        return Ok(None);
    };
    let read: Vec<NeverCaptureApp> = items.iter().filter_map(read_app).collect();
    if read.len() == items.len() {
        return Ok(Some(read));
    }
    let mut list = default_never_capture();
    list.extend(read);
    Ok(Some(list))
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

/// Password managers on macOS, by bundle id (lower case). An entry
/// also covers the app's helpers ([`crate::vision::id_matches`]).
///
/// Where each id comes from is noted, checked on 2026-11-11:
/// "cask" is the Homebrew cask of the app (the ids in its `uninstall`
/// and `zap` stanzas), "App Store" is the bundle id Apple's store
/// lists for the Mac app. An id without such a source is marked best
/// effort: if it is wrong it matches nothing and costs nothing.
const MAC_NEVER_CAPTURE: &[(&str, &str)] = &[
    // cask `1password`. Its launcher, `com.1password.1password-launcher`,
    // is covered as a helper.
    ("com.1password.1password", "1Password"),
    // App Store, "1Password 7 - Password Manager".
    ("com.agilebits.onepassword7", "1Password 7"),
    // cask `bitwarden`, and App Store.
    ("com.bitwarden.desktop", "Bitwarden"),
    // cask `keepassxc`.
    ("org.keepassxc.keepassxc", "KeePassXC"),
    // cask `lastpass`.
    ("com.lastpass.lastpassmacdesktop", "LastPass"),
    // App Store, "Dashlane Password Manager".
    ("com.dashlane.dashlanephonefinal", "Dashlane"),
    // Best effort: the older Dashlane desktop app.
    ("com.dashlane.dashlane", "Dashlane"),
    // Best effort (Apple's own apps are in no cask and no store
    // listing): Passwords on macOS 15 and later, and Keychain Access.
    ("com.apple.passwords", "Passwords"),
    ("com.apple.keychainaccess", "Keychain Access"),
    // cask `proton-pass` (the desktop app), and App Store, "Proton
    // Pass for Safari".
    ("me.proton.pass.electron", "Proton Pass"),
    ("me.proton.pass.catalyst", "Proton Pass"),
    // cask `enpass`, and App Store.
    ("in.sinew.enpass-desktop", "Enpass"),
    // cask `nordpass` (the desktop app), and App Store, "NordPass
    // Password Manager".
    ("com.nordsec.nordpass", "NordPass"),
    ("com.nordpass.safari.app.password.manager", "NordPass"),
    // cask `keeper-password-manager` (the desktop app), and App
    // Store, "Keeper Password Manager".
    ("com.keepersecurity.passwordmanager", "Keeper"),
    ("com.callpod.keepermac.lite", "Keeper"),
    // App Store, "Strongbox - Password Manager". Also covers
    // `com.markmcguill.strongbox.mac` as a helper id.
    ("com.markmcguill.strongbox", "Strongbox"),
];

/// Password managers on Windows, by exe file name (lower case).
///
/// "Scoop" is the exe the app's manifest in the Scoop `extras` bucket
/// starts, checked on 2026-11-11. The others are best effort: the
/// name the vendor's installer is known to use, not confirmed against
/// a source. None of them has been seen running on a real machine.
const WINDOWS_NEVER_CAPTURE: &[(&str, &str)] = &[
    // Best effort.
    ("1password.exe", "1Password"),
    // Scoop `bitwarden`.
    ("bitwarden.exe", "Bitwarden"),
    // Scoop `keepassxc`.
    ("keepassxc.exe", "KeePassXC"),
    // Best effort.
    ("lastpass.exe", "LastPass"),
    // Best effort.
    ("dashlane.exe", "Dashlane"),
    // Scoop `keepass`, and keepass.info.
    ("keepass.exe", "KeePass"),
    // Scoop `proton-pass`.
    ("protonpass.exe", "Proton Pass"),
    // Scoop `enpass`.
    ("enpass.exe", "Enpass"),
    // Scoop `nordpass`.
    ("nordpass.exe", "NordPass"),
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
    fn the_default_trigger_is_phrases_on_macos_and_off_on_windows() {
        assert_eq!(default_trigger_on(false), VisionTrigger::Phrases);
        // Not verified on a real Windows machine yet: off until the
        // person switches it on.
        assert_eq!(default_trigger_on(true), VisionTrigger::Off);
        assert_eq!(default_trigger(), default_trigger_on(cfg!(target_os = "windows")));
        // The block a person never touched, and a block without a
        // trigger, both get it.
        assert_eq!(VisionSettings::default().trigger, default_trigger());
        assert_eq!(VisionTrigger::default(), default_trigger());
        let s: VisionSettings = serde_json::from_str(r#"{"phrases":["look here"]}"#).unwrap();
        assert_eq!(s.trigger, default_trigger());
        // A trigger that was chosen holds on every platform.
        let s: VisionSettings = serde_json::from_str(r#"{"trigger":"phrases"}"#).unwrap();
        assert_eq!(s.trigger, VisionTrigger::Phrases);
    }

    #[test]
    fn defaults_are_the_platform_trigger_with_the_built_in_lists() {
        let s = VisionSettings::default();
        assert_eq!(s.trigger, default_trigger());
        assert_eq!((s.phrases.clone(), s.never_capture.clone()), (None, None));
        assert_eq!(s.clone().cleaned(), s);
        assert_eq!(s.effective_phrases(), default_phrases());
        assert_eq!(s.effective_never_capture(), default_never_capture());
        // Nothing of the defaults is written out: a later version's
        // defaults reach a person who never edited a list.
        assert_eq!(
            serde_json::to_string(&s).unwrap(),
            format!(r#"{{"trigger":"{}","phrases":null,"never_capture":null}}"#, default_trigger().as_str())
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
            "me.proton.pass.electron",
            "me.proton.pass.catalyst",
            "in.sinew.enpass-desktop",
            "com.nordsec.nordpass",
            "com.nordpass.safari.app.password.manager",
            "com.keepersecurity.passwordmanager",
            "com.callpod.keepermac.lite",
            "com.markmcguill.strongbox",
        ] {
            assert!(mac.contains(&id), "missing {id}");
        }
        let windows: Vec<&str> = WINDOWS_NEVER_CAPTURE.iter().map(|(id, _)| *id).collect();
        assert_eq!(
            windows,
            [
                "1password.exe",
                "bitwarden.exe",
                "keepassxc.exe",
                "lastpass.exe",
                "dashlane.exe",
                "keepass.exe",
                "protonpass.exe",
                "enpass.exe",
                "nordpass.exe",
            ]
        );
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
        assert_eq!(s.never_capture, None);

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

        // Phrases of the wrong shape are skipped (a phrase is not a
        // protection); an app id alone is fine.
        let s: VisionSettings = serde_json::from_str(
            r#"{"phrases":["on my screen",5,null],"never_capture":["Bank.App",{"id":"com.example.vault","name":"Vault"}]}"#,
        )
        .unwrap();
        let s = s.cleaned();
        assert_eq!(s.phrases.unwrap(), ["on my screen"]);
        assert_eq!(
            s.never_capture.unwrap(),
            [NeverCaptureApp::new("bank.app", ""), NeverCaptureApp::new("com.example.vault", "Vault")]
        );
    }

    /// The never-capture list as `raw` reads, tidied.
    fn never_capture_of(raw: &str) -> Vec<NeverCaptureApp> {
        let s: VisionSettings = serde_json::from_str(&format!(r#"{{"never_capture":{raw}}}"#)).unwrap();
        s.cleaned().effective_never_capture()
    }

    #[test]
    fn a_list_with_an_entry_that_cannot_be_read_never_protects_fewer_apps_than_the_built_in_one() {
        let defaults = default_never_capture();
        // A typo in the key, a number, an empty id: each used to leave
        // a list with no app on it at all.
        for raw in [
            r#"[{"bundle_id":"com.1password.1password"}]"#,
            "[17]",
            "[null]",
            r#"[{"name":"no id"}]"#,
            r#"[{"id":7}]"#,
            r#"[{"id":"  "}]"#,
            r#"[""]"#,
            "[[]]",
            r#"[{"bundle_id":"com.example.vault"},17,true]"#,
        ] {
            assert_eq!(never_capture_of(raw), defaults, "{raw}");
        }
        // Not a list at all: the built-in list, and stored as "never
        // edited".
        for raw in ["null", "7", r#""com.example.vault""#, r#"{"id":"com.example.vault"}"#] {
            let s: VisionSettings = serde_json::from_str(&format!(r#"{{"never_capture":{raw}}}"#)).unwrap();
            assert_eq!(s.never_capture, None, "{raw}");
            assert_eq!(s.effective_never_capture(), defaults, "{raw}");
        }
        // What could be read is kept, after the built-in apps.
        let mixed = never_capture_of(
            r#"["Bank.App",{"id":"com.example.vault","name":"Vault"},4,{"name":"no id"},{"id":"notes.exe","name":9}]"#,
        );
        assert_eq!(mixed[..defaults.len()], defaults[..]);
        assert_eq!(
            mixed[defaults.len()..],
            [
                NeverCaptureApp::new("bank.app", ""),
                NeverCaptureApp::new("com.example.vault", "Vault"),
                // A name of the wrong type costs the name only.
                NeverCaptureApp::new("notes.exe", ""),
            ]
        );
        // An entry that is already built in is not listed twice.
        let twice = never_capture_of(&format!(r#"["{}",17]"#, defaults[0].id));
        assert_eq!(twice, defaults);
    }

    #[test]
    fn only_a_well_formed_empty_list_protects_nothing() {
        assert_eq!(never_capture_of("[]"), Vec::<NeverCaptureApp>::new());
        // A list that reads in full is the person's own, however
        // short.
        assert_eq!(never_capture_of(r#"["bank.app"]"#), [NeverCaptureApp::new("bank.app", "")]);
        // It survives a save and a load as what it is.
        let emptied = VisionSettings { never_capture: Some(Vec::new()), ..VisionSettings::default() };
        let back: VisionSettings = serde_json::from_str(&serde_json::to_string(&emptied).unwrap()).unwrap();
        assert_eq!(back.never_capture, Some(Vec::new()));
        // And what core writes always reads back as written.
        let own = VisionSettings {
            never_capture: Some(vec![NeverCaptureApp::new("com.example.vault", "Vault")]),
            ..VisionSettings::default()
        };
        let back: VisionSettings = serde_json::from_str(&serde_json::to_string(&own).unwrap()).unwrap();
        assert_eq!(back, own);
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
