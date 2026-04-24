//! Post-process raw Parakeet transcripts before injection.
//!
//! Mirrors the Swift `TranscriptProcessor` exactly — replacing the Swift
//! pipeline with a Rust call should be zero behavior diff. Three passes:
//!   1. Strip filler words (um/uh/øh/…) using a per-language register.
//!   2. Apply user substitutions ("open whisper" → "OpenWhisper").
//!   3. Normalize whitespace (collapse runs, strip before punctuation,
//!      merge comma runs left by filler removal).
//!
//! The EN/DA asymmetry around "er" is deliberate: "er" is English
//! hesitation but also the Danish copula ("det er fedt"). Stripping
//! it blindly ate Danish verbs.

use std::sync::LazyLock;

use regex::{Regex, RegexBuilder};

pub enum FillerLang {
    En,
    Da,
}

const EN_FILLERS: &[&str] = &[
    "um", "umm", "ummm",
    "uh", "uhh", "uhhh",
    "uhm", "uhmm",
    "er", "err", "erm", "errm",
    "ah", "ahh",
    "oh", "ooh",
    "hm", "hmm", "hmmm",
    "mm", "mmm",
];

const DA_FILLERS: &[&str] = &[
    "um", "umm", "ummm",
    "uh", "uhh", "uhhh",
    "uhm", "uhmm",
    "err", "erm", "errm",
    "ah", "ahh",
    "oh", "ooh",
    "hm", "hmm", "hmmm",
    "mm", "mmm",
    "øh", "øhm",
];

const SUBSTITUTIONS: &[(&str, &str)] = &[
    ("open whisper", "OpenWhisper"),
];

static EN_FILLER_RE: LazyLock<Regex> = LazyLock::new(|| build_filler_regex(EN_FILLERS));
static DA_FILLER_RE: LazyLock<Regex> = LazyLock::new(|| build_filler_regex(DA_FILLERS));
static MULTI_SPACE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[ \t]{2,}").unwrap());
static SPACE_BEFORE_PUNCT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r" ([,.!?;:])").unwrap());
static REPEATED_COMMAS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(,\s*){2,}").unwrap());
static SUB_REGEXES: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    let mut ordered: Vec<(&str, &str)> = SUBSTITUTIONS.to_vec();
    ordered.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    ordered
        .into_iter()
        .map(|(key, value)| {
            let pat = format!(r"\b{}\b", regex::escape(key));
            let re = RegexBuilder::new(&pat)
                .case_insensitive(true)
                .build()
                .expect("valid substitution regex");
            (re, value)
        })
        .collect()
});

fn build_filler_regex(words: &[&str]) -> Regex {
    let mut sorted: Vec<&str> = words.to_vec();
    sorted.sort_by(|a, b| b.len().cmp(&a.len()));
    let alts: Vec<String> = sorted.iter().map(|w| regex::escape(w)).collect();
    let pattern = format!(r"\b(?:{})\b,?\s*", alts.join("|"));
    RegexBuilder::new(&pattern)
        .case_insensitive(true)
        .build()
        .expect("valid filler regex")
}

pub fn detect_lang(text: &str) -> FillerLang {
    for c in text.chars() {
        if matches!(c, 'æ' | 'ø' | 'å' | 'Æ' | 'Ø' | 'Å') {
            return FillerLang::Da;
        }
    }
    FillerLang::En
}

pub fn process(text: &str) -> String {
    let lang = detect_lang(text);
    let out = strip_fillers(text, &lang);
    let out = apply_subs(&out);
    normalize_whitespace(&out)
}

fn strip_fillers(text: &str, lang: &FillerLang) -> String {
    let re = match lang {
        FillerLang::En => &*EN_FILLER_RE,
        FillerLang::Da => &*DA_FILLER_RE,
    };
    re.replace_all(text, "").into_owned()
}

fn apply_subs(text: &str) -> String {
    let mut out = text.to_string();
    for (re, value) in SUB_REGEXES.iter() {
        out = re.replace_all(&out, *value).into_owned();
    }
    out
}

fn normalize_whitespace(text: &str) -> String {
    let out = MULTI_SPACE.replace_all(text, " ");
    let out = SPACE_BEFORE_PUNCT.replace_all(&out, "$1");
    let out = REPEATED_COMMAS.replace_all(&out, ", ");
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_english_fillers() {
        assert_eq!(process("um, this is a test"), "this is a test");
        assert_eq!(process("uh well uhm I think"), "well I think");
    }

    #[test]
    fn strips_danish_oh_filler() {
        // "øh" triggers DA detection AND is a DA filler.
        assert_eq!(process("det er fedt øh"), "det er fedt");
    }

    #[test]
    fn danish_preserves_er_copula() {
        // "æ" forces DA detection; "er" is not in DA_FILLERS.
        assert_eq!(process("jeg er træt"), "jeg er træt");
    }

    #[test]
    fn substitutes_open_whisper() {
        assert_eq!(process("open whisper rocks"), "OpenWhisper rocks");
        assert_eq!(process("Open Whisper rocks"), "OpenWhisper rocks");
    }

    #[test]
    fn normalizes_whitespace() {
        assert_eq!(process("hello   world"), "hello world");
        assert_eq!(process("hello ,world"), "hello,world");
    }

    #[test]
    fn collapses_repeated_commas_from_filler_strip() {
        assert_eq!(process("um, uh, hello"), "hello");
    }

    #[test]
    fn empty_string_passes_through() {
        assert_eq!(process(""), "");
    }

    #[test]
    fn pure_filler_input() {
        assert_eq!(process("um uh ah"), "");
    }
}
