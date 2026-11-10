//! User dictionary — names, brands and jargon the recognizer gets wrong.
//!
//! Parakeet has no hot-word biasing, so the dictionary is a post-pass
//! over the transcript (see [`crate::transcript::process`]). Each entry
//! is a canonical `term` plus the ways it was misheard (`heard_as`):
//!
//! ```json
//! {
//!   "terms": [
//!     { "term": "PayProff", "heard_as": ["pay prof"] },
//!     { "term": "Emanoil", "heard_as": ["Immanuel", "Emanuel"] }
//!   ]
//! }
//! ```
//!
//! Matching is case-insensitive and whole-word. Without any `heard_as`,
//! a term still fixes its own casing ("payproff" → "PayProff") and, for
//! a camel-case term, its split form ("pay proff", "pay-proff").
//! A space in a pattern matches spaces, hyphens or nothing, so "pay
//! proof" also catches "payproof".
//!
//! Storage: one pretty-printed JSON file (`dictionary.json` next to
//! `settings.json`). A plain file on purpose: people and agents edit it
//! by hand, and the file is re-read whenever its modification time or
//! size changes, so an edit applies to the next dictation without a
//! restart. A file that fails to parse keeps the last good dictionary.
//!
//! The shell calls [`set_path`] once at boot. Until then (and for
//! callers that never set one, like unit tests) only the built-in terms
//! apply.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::SystemTime;

use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

/// File name inside the shell's config dir.
pub const FILE_NAME: &str = "dictionary.json";

/// Instructions for AI agents that read and update the dictionary,
/// written as `AGENTS.md` next to it (see [`set_path`]). Tool-neutral:
/// Codex and others read `AGENTS.md` by convention, and Claude imports
/// it from the user's CLAUDE.md. Shipped with the app so the text
/// always matches the installed format.
pub const AGENTS_GUIDE: &str = include_str!("../assets/dictionary-agents.md");

/// File name of [`AGENTS_GUIDE`] next to the dictionary.
pub const AGENTS_FILE_NAME: &str = "AGENTS.md";

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct Dictionary {
    #[serde(default)]
    pub terms: Vec<DictionaryTerm>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct DictionaryTerm {
    /// The spelling to write, e.g. "PayProff".
    pub term: String,
    /// What the recognizer wrote instead, e.g. "pay prof".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub heard_as: Vec<String>,
}

/// One dictionary fix in a transcript: `from` is what the recognizer
/// wrote, `to` the dictionary term that replaced it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Replacement {
    pub from: String,
    pub to: String,
}

/// Terms that ship with the app. User terms win on conflicts.
const BUILTIN_TERMS: &[&str] = &["OpenWhisper"];

// --- Matching ------------------------------------------------------

/// Compiled form: one alternation regex plus a lookup from the compact
/// match text (lowercase, no spaces or hyphens) to its replacement. A
/// single pass means a replacement is never re-matched by another
/// entry.
pub struct Matcher {
    re: Option<Regex>,
    replacements: HashMap<String, String>,
    /// Canonical spellings (built-in and user terms), in file order.
    terms: Vec<String>,
    /// The same terms as the Refine prompt lists them: with what the
    /// recognizer wrote instead, e.g. `Jeppe [misheard as: yeb, Jebbe]`.
    prompt_terms: Vec<String>,
}

impl Matcher {
    pub fn new(dict: &Dictionary) -> Self {
        let mut replacements: HashMap<String, String> = HashMap::new();
        // Spaced, normalized patterns for the regex ("pay proof").
        let mut patterns: Vec<String> = Vec::new();
        let mut terms: Vec<String> = Vec::new();
        let mut prompt_terms: Vec<String> = Vec::new();
        let mut add = |pattern: &str, term: &str| {
            let spaced = normalize(pattern);
            if spaced.is_empty() {
                return;
            }
            replacements.insert(compact(&spaced), term.to_string());
            if !patterns.contains(&spaced) {
                patterns.push(spaced);
            }
        };

        let builtin = BUILTIN_TERMS.iter().map(|t| DictionaryTerm {
            term: (*t).to_string(),
            heard_as: Vec::new(),
        });
        // Built-ins first so user entries overwrite them in the map.
        for entry in builtin.chain(dict.terms.iter().cloned()) {
            let term = entry.term.trim();
            if term.is_empty() {
                continue;
            }
            if !terms.iter().any(|t| t == term) {
                terms.push(term.to_string());
                let heard: Vec<&str> = entry
                    .heard_as
                    .iter()
                    .map(|h| h.trim())
                    .filter(|h| !h.is_empty())
                    .collect();
                prompt_terms.push(if heard.is_empty() {
                    term.to_string()
                } else {
                    format!("{term} [misheard as: {}]", heard.join(", "))
                });
            }
            add(term, term);
            let split = split_camel(term);
            if split != term {
                add(&split, term);
            }
            for heard in &entry.heard_as {
                add(heard, term);
            }
        }

        if patterns.is_empty() {
            return Self {
                re: None,
                replacements,
                terms,
                prompt_terms,
            };
        }

        // Longest first so "pay proff app" beats "pay proff".
        patterns.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
        let alts: Vec<String> = patterns.iter().map(|k| pattern_for(k)).collect();
        let re = RegexBuilder::new(&format!("(?:{})", alts.join("|")))
            .case_insensitive(true)
            .build()
            .ok();
        Self {
            re,
            replacements,
            terms,
            prompt_terms,
        }
    }

    /// [`Matcher::terms`] with each term's known mis-hearings, for the
    /// Refine prompt. The rule pass only fixes the exact mis-hearings
    /// listed in the dictionary; shown these examples, the model can
    /// also catch a new one that sounds alike ("Yebbe" when "yeb" and
    /// "Jebbe" are listed for Jeppe).
    pub fn prompt_terms(&self) -> &[String] {
        &self.prompt_terms
    }

    /// The spellings this dictionary enforces ("PayProff", "Emanoil"),
    /// built-ins included. The Refine prompt lists them so a language
    /// model does not "correct" them back (see [`crate::refine`]).
    pub fn terms(&self) -> &[String] {
        &self.terms
    }

    pub fn apply(&self, text: &str) -> String {
        self.apply_tracked(text).0
    }

    /// Like [`Matcher::apply`], plus the fixes it made, in order of
    /// first appearance. A match already spelled like its term (e.g.
    /// "PayProff" itself) is not a fix and is left out; repeats of the
    /// same fix are listed once.
    pub fn apply_tracked(&self, text: &str) -> (String, Vec<Replacement>) {
        let Some(re) = &self.re else {
            return (text.to_string(), Vec::new());
        };
        let mut fixes: Vec<Replacement> = Vec::new();
        let out = re
            .replace_all(text, |caps: &regex::Captures| {
                let found = &caps[0];
                let Some(term) = self.replacements.get(&compact(found)) else {
                    return found.to_string();
                };
                if found != term {
                    let fix = Replacement {
                        from: found.to_string(),
                        to: term.clone(),
                    };
                    if !fixes.contains(&fix) {
                        fixes.push(fix);
                    }
                }
                term.clone()
            })
            .into_owned();
        (out, fixes)
    }
}

/// Lookup key: lowercase without spaces or hyphens, since a space in a
/// pattern may match none ("pay proof" finds "payproof").
fn compact(s: &str) -> String {
    normalize(s).replace(' ', "")
}

/// Lowercase, hyphens and whitespace runs collapsed to one space.
fn normalize(s: &str) -> String {
    s.split(|c: char| c.is_whitespace() || c == '-')
        .filter(|w| !w.is_empty())
        .map(|w| w.to_lowercase())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Regex for one normalized key: words joined by `[\s-]*` (so "pay
/// prof" also matches "pay-prof" and "payprof"), with a word boundary
/// only on edges that are word characters (so "C++" or ".NET" still
/// match).
fn pattern_for(key: &str) -> String {
    let body = key
        .split(' ')
        .map(regex::escape)
        .collect::<Vec<_>>()
        .join(r"[\s-]*");
    let is_word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
    let start = if is_word(key.chars().next()) { r"\b" } else { "" };
    let end = if is_word(key.chars().last()) { r"\b" } else { "" };
    format!("{start}{body}{end}")
}

/// "PayProff" → "Pay Proff", "OpenWhisper" → "Open Whisper". Acronym
/// runs stay together ("HeroDesk" → "Hero Desk", "MCPServer" → "MCP
/// Server").
fn split_camel(term: &str) -> String {
    let chars: Vec<char> = term.chars().collect();
    let mut out = String::with_capacity(term.len() + 4);
    for (i, &c) in chars.iter().enumerate() {
        if i > 0 && c.is_uppercase() {
            let prev = chars[i - 1];
            let next_lower = chars.get(i + 1).is_some_and(|n| n.is_lowercase());
            if prev.is_lowercase() || (prev.is_uppercase() && next_lower) {
                out.push(' ');
            }
        }
        out.push(c);
    }
    out
}

// --- Process-wide state with mtime reload --------------------------

#[derive(PartialEq, Eq, Clone, Copy)]
struct Stamp {
    modified: Option<SystemTime>,
    len: u64,
}

struct State {
    path: Option<PathBuf>,
    stamp: Option<Stamp>,
    matcher: Arc<Matcher>,
}

static STATE: LazyLock<Mutex<State>> = LazyLock::new(|| {
    Mutex::new(State {
        path: None,
        stamp: None,
        matcher: Arc::new(Matcher::new(&Dictionary::default())),
    })
});

/// Point the process at the dictionary file. Loads it immediately when
/// it exists; a missing file means built-in terms only. Also writes
/// [`AGENTS_GUIDE`] next to it, so agents find the instructions with
/// the file.
pub fn set_path(path: &Path) {
    if let Err(e) = write_agents_guide(path) {
        eprintln!("[ow.dictionary] {e}");
    }
    if let Ok(mut s) = STATE.lock() {
        s.path = Some(path.to_path_buf());
        s.stamp = None;
        refresh(&mut s);
    }
}

/// The configured file path, if the shell set one.
pub fn path() -> Option<PathBuf> {
    STATE.lock().ok().and_then(|s| s.path.clone())
}

/// Current matcher, re-reading the file first when it changed on disk.
/// One `stat` per call; called once per dictation.
pub fn current_matcher() -> Arc<Matcher> {
    match STATE.lock() {
        Ok(mut s) => {
            refresh(&mut s);
            s.matcher.clone()
        }
        Err(_) => Arc::new(Matcher::new(&Dictionary::default())),
    }
}

fn refresh(s: &mut State) {
    let Some(path) = s.path.clone() else {
        return;
    };
    let stamp = fs::metadata(&path).ok().map(|m| Stamp {
        modified: m.modified().ok(),
        len: m.len(),
    });
    if stamp == s.stamp {
        return;
    }
    match stamp {
        None => {
            // File removed or never created: built-ins only.
            s.matcher = Arc::new(Matcher::new(&Dictionary::default()));
        }
        Some(_) => match read(&path) {
            Ok(dict) => s.matcher = Arc::new(Matcher::new(&dict)),
            Err(e) => {
                // Keep the last good matcher; a half-written or broken
                // file must not wipe the user's corrections.
                eprintln!("[ow.dictionary] {e}; keeping the previous dictionary");
            }
        },
    }
    s.stamp = stamp;
}

// --- IO ------------------------------------------------------------

/// Write `AGENTS.md` next to the dictionary when it is missing or
/// differs from the shipped text (an app update). Creates the folder.
pub fn write_agents_guide(dictionary_path: &Path) -> Result<(), String> {
    let Some(dir) = dictionary_path.parent() else {
        return Ok(());
    };
    let target = dir.join(AGENTS_FILE_NAME);
    if fs::read_to_string(&target).is_ok_and(|s| s == AGENTS_GUIDE) {
        return Ok(());
    }
    if !dir.as_os_str().is_empty() {
        fs::create_dir_all(dir).map_err(|e| format!("create config dir: {e}"))?;
    }
    fs::write(&target, AGENTS_GUIDE).map_err(|e| format!("write {}: {e}", target.display()))
}

fn read(path: &Path) -> Result<Dictionary, String> {
    let bytes = fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("parse {}: {e}", path.display()))
}

/// Load the dictionary at `path`. Missing file → empty dictionary.
pub fn load(path: &Path) -> Result<Dictionary, String> {
    if !path.exists() {
        return Ok(Dictionary::default());
    }
    read(path)
}

/// Write the dictionary (trimmed, empty entries dropped, sorted by
/// term) via a temp file + rename so a reader never sees half a file.
///
/// When `path` is a symlink (e.g. the dev build's file linked to the
/// release build's, so both apps share one dictionary), the write goes
/// to the link's target and the link stays in place.
pub fn save(path: &Path, dict: &Dictionary) -> Result<Dictionary, String> {
    let clean = clean(dict);
    let target = resolve_link(path);
    let path = target.as_path();
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| format!("create config dir: {e}"))?;
        }
    }
    let mut bytes = serde_json::to_vec_pretty(&clean).map_err(|e| format!("serialize: {e}"))?;
    bytes.push(b'\n');
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, bytes).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| format!("rename to {}: {e}", path.display()))?;
    Ok(clean)
}

/// Follow symlinks so a rename replaces the target, not the link. A
/// dangling link resolves to its target path (created on write).
fn resolve_link(path: &Path) -> PathBuf {
    if let Ok(real) = fs::canonicalize(path) {
        return real;
    }
    match fs::read_link(path) {
        Ok(target) if target.is_absolute() => target,
        Ok(target) => path.parent().map_or(target.clone(), |p| p.join(&target)),
        Err(_) => path.to_path_buf(),
    }
}

fn clean(dict: &Dictionary) -> Dictionary {
    let mut terms: Vec<DictionaryTerm> = Vec::new();
    for entry in &dict.terms {
        let term = entry.term.trim().to_string();
        if term.is_empty() {
            continue;
        }
        let mut heard_as: Vec<String> = Vec::new();
        for h in &entry.heard_as {
            let h = h.trim().to_string();
            if !h.is_empty()
                && compact(&h) != compact(&term)
                && !heard_as.iter().any(|x| compact(x) == compact(&h))
            {
                heard_as.push(h);
            }
        }
        // Same term twice: merge the heard_as lists.
        if let Some(existing) = terms
            .iter_mut()
            .find(|t| t.term.to_lowercase() == term.to_lowercase())
        {
            for h in heard_as {
                if !existing.heard_as.iter().any(|x| compact(x) == compact(&h)) {
                    existing.heard_as.push(h);
                }
            }
            continue;
        }
        terms.push(DictionaryTerm { term, heard_as });
    }
    terms.sort_by_key(|t| t.term.to_lowercase());
    Dictionary { terms }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dict(entries: &[(&str, &[&str])]) -> Dictionary {
        Dictionary {
            terms: entries
                .iter()
                .map(|(t, h)| DictionaryTerm {
                    term: (*t).to_string(),
                    heard_as: h.iter().map(|s| (*s).to_string()).collect(),
                })
                .collect(),
        }
    }

    #[test]
    fn prompt_terms_carry_the_known_mishearings() {
        let m = Matcher::new(&dict(&[
            ("Jeppe", &["yeb", "Jebbe"]),
            ("HeroDesk", &[]),
        ]));
        // Built-in first, then the user's, each once.
        assert_eq!(m.terms(), ["OpenWhisper", "Jeppe", "HeroDesk"]);
        assert_eq!(
            m.prompt_terms(),
            ["OpenWhisper", "Jeppe [misheard as: yeb, Jebbe]", "HeroDesk"]
        );
    }

    #[test]
    fn fixes_casing_and_split_form_without_aliases() {
        let m = Matcher::new(&dict(&[("PayProff", &[])]));
        assert_eq!(m.apply("launch payproff today"), "launch PayProff today");
        assert_eq!(m.apply("launch Pay Proff today"), "launch PayProff today");
        assert_eq!(m.apply("launch pay-proff today"), "launch PayProff today");
    }

    #[test]
    fn replaces_heard_as_whole_words_only() {
        let m = Matcher::new(&dict(&[("Emanoil", &["Immanuel", "Emanuel"])]));
        assert_eq!(m.apply("ask Immanuel and emanuel"), "ask Emanoil and Emanoil");
        assert_eq!(m.apply("Emanuelle stays"), "Emanuelle stays");
    }

    #[test]
    fn longest_match_wins_and_no_cascade() {
        let m = Matcher::new(&dict(&[
            ("PayProff", &["pay prof"]),
            ("PayProff website", &["pay prof website"]),
            ("Jeppe", &["yeb"]),
        ]));
        assert_eq!(m.apply("the pay prof website"), "the PayProff website");
        assert_eq!(m.apply("yeb said pay prof"), "Jeppe said PayProff");
    }

    #[test]
    fn spaces_in_patterns_also_match_no_space() {
        let m = Matcher::new(&dict(&[("PayProff", &["pay proof"])]));
        assert_eq!(m.apply("in payproof today"), "in PayProff today");
        assert_eq!(m.apply("in Pay-Proof today"), "in PayProff today");
        // Still whole words only.
        assert_eq!(m.apply("prepayproof"), "prepayproof");
    }

    #[test]
    fn tracks_fixes_once_and_skips_correct_spellings() {
        let m = Matcher::new(&dict(&[
            ("PayProff", &["pay proof"]),
            ("Emanoil", &["Immanuel"]),
        ]));
        let (out, fixes) =
            m.apply_tracked("Immanuel and payproof, PayProff, then Immanuel again");
        assert_eq!(out, "Emanoil and PayProff, PayProff, then Emanoil again");
        assert_eq!(
            fixes,
            vec![
                Replacement { from: "Immanuel".into(), to: "Emanoil".into() },
                Replacement { from: "payproof".into(), to: "PayProff".into() },
            ]
        );
    }

    #[test]
    fn keeps_surrounding_punctuation() {
        let m = Matcher::new(&dict(&[("Jeppe", &["hipp"])]));
        assert_eq!(m.apply("Hipp, are we ready?"), "Jeppe, are we ready?");
    }

    #[test]
    fn non_word_edges_match() {
        let m = Matcher::new(&dict(&[(".NET", &["dot net"])]));
        assert_eq!(m.apply("a dot net app"), "a .NET app");
    }

    #[test]
    fn builtin_open_whisper() {
        let m = Matcher::new(&Dictionary::default());
        assert_eq!(m.apply("open whisper rocks"), "OpenWhisper rocks");
    }

    #[test]
    fn split_camel_cases() {
        assert_eq!(split_camel("PayProff"), "Pay Proff");
        assert_eq!(split_camel("HeroDesk"), "Hero Desk");
        assert_eq!(split_camel("MCPServer"), "MCP Server");
        assert_eq!(split_camel("Jeppe"), "Jeppe");
    }

    #[test]
    fn save_cleans_merges_and_sorts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        let saved = save(
            &path,
            &dict(&[
                ("  PayProff ", &["pay prof", "", "payproff"]),
                ("Emanoil", &["Immanuel"]),
                ("payproff", &["pay proof", "Pay Prof"]),
                ("", &["ignored"]),
            ]),
        )
        .unwrap();
        assert_eq!(
            saved,
            dict(&[
                ("Emanoil", &["Immanuel"]),
                ("PayProff", &["pay prof", "pay proof"]),
            ])
        );
        assert_eq!(load(&path).unwrap(), saved);
    }

    #[cfg(unix)]
    #[test]
    fn save_through_a_symlink_keeps_the_link() {
        let dir = tempfile::tempdir().unwrap();
        let shared = dir.path().join("release").join(FILE_NAME);
        let dev_dir = dir.path().join("dev");
        fs::create_dir_all(&dev_dir).unwrap();
        let link = dev_dir.join(FILE_NAME);
        // Dangling at first: the release file doesn't exist yet.
        std::os::unix::fs::symlink(&shared, &link).unwrap();

        save(&link, &dict(&[("Jeppe", &["hipp"])])).unwrap();
        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(load(&shared).unwrap(), dict(&[("Jeppe", &["hipp"])]));

        save(&link, &dict(&[("Emanoil", &[])])).unwrap();
        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(load(&shared).unwrap(), dict(&[("Emanoil", &[])]));
    }

    #[test]
    fn writes_agents_guide_next_to_the_dictionary() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config").join(FILE_NAME);
        write_agents_guide(&path).unwrap();
        let guide = dir.path().join("config").join(AGENTS_FILE_NAME);
        assert_eq!(fs::read_to_string(&guide).unwrap(), AGENTS_GUIDE);

        // An outdated copy is replaced.
        fs::write(&guide, "old").unwrap();
        write_agents_guide(&path).unwrap();
        assert_eq!(fs::read_to_string(&guide).unwrap(), AGENTS_GUIDE);
    }

    #[test]
    fn reloads_after_file_changes_and_survives_bad_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        let mut state = State {
            path: Some(path.clone()),
            stamp: None,
            matcher: Arc::new(Matcher::new(&Dictionary::default())),
        };
        refresh(&mut state);
        assert_eq!(state.matcher.apply("hipp"), "hipp");

        save(&path, &dict(&[("Jeppe", &["hipp"])])).unwrap();
        refresh(&mut state);
        assert_eq!(state.matcher.apply("hipp"), "Jeppe");

        fs::write(&path, b"{ not json").unwrap();
        refresh(&mut state);
        assert_eq!(state.matcher.apply("hipp"), "Jeppe");
    }
}
