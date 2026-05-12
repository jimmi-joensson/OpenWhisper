//! Post-process raw Parakeet transcripts before injection.
//!
//! Four passes:
//!   1. Strip filler words (um/uh/øh/…) using a per-language register.
//!   2. Apply user substitutions ("open whisper" → "OpenWhisper").
//!   3. Collapse adjacent duplicate words ("let's let's" → "let's").
//!      Punctuation between words protects intentional repetition
//!      ("really, really nice" stays; "no. No problem" stays).
//!   4. Normalize whitespace (collapse runs, strip before punctuation,
//!      merge comma runs left by filler removal).
//!
//! After the four passes, append a single trailing space when the result
//! is non-empty so consecutive injections into the same input don't run
//! together (e.g. "hello world" + "how are you" → "hello world how are
//! you", not "hello worldhow are you"). Empty results stay empty so a
//! pure-filler dictation doesn't write a lone space.
//!
//! The EN/DA asymmetry around "er" is deliberate: "er" is English
//! hesitation but also the Danish copula ("det er fedt"). Stripping
//! it blindly ate Danish verbs.

use std::sync::LazyLock;

use regex::{Regex, RegexBuilder};

#[non_exhaustive]
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
    "hm", "hmm", "hmmm",
    "mm", "mmm",
];

const DA_FILLERS: &[&str] = &[
    "um", "umm", "ummm",
    "uh", "uhh", "uhhh",
    "uhm", "uhmm",
    "err", "erm", "errm",
    "ah", "ahh",
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
    let out = dedupe_repeats(&out);
    let out = normalize_whitespace(&out);
    if out.is_empty() {
        out
    } else {
        format!("{out} ")
    }
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

// Collapse runs of the same word separated by whitespace only. Punctuation
// glued to either token (e.g. "really," or "No.") breaks the match, so
// rhetorical doubling and sentence-boundary repeats survive. Comparison is
// case-insensitive; first occurrence's casing wins.
fn dedupe_repeats(text: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    let mut prev_lower: Option<String> = None;
    for tok in text.split_whitespace() {
        let lower = tok.to_lowercase();
        if prev_lower.as_deref() == Some(lower.as_str()) {
            continue;
        }
        prev_lower = Some(lower);
        out.push(tok);
    }
    out.join(" ")
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
        assert_eq!(process("um, this is a test"), "this is a test ");
        assert_eq!(process("uh well uhm I think"), "well I think ");
    }

    #[test]
    fn strips_danish_oh_filler() {
        // "øh" triggers DA detection AND is a DA filler.
        assert_eq!(process("det er fedt øh"), "det er fedt ");
    }

    #[test]
    fn danish_preserves_er_copula() {
        // "æ" forces DA detection; "er" is not in DA_FILLERS.
        assert_eq!(process("jeg er træt"), "jeg er træt ");
    }

    #[test]
    fn substitutes_open_whisper() {
        assert_eq!(process("open whisper rocks"), "OpenWhisper rocks ");
        assert_eq!(process("Open Whisper rocks"), "OpenWhisper rocks ");
    }

    #[test]
    fn normalizes_whitespace() {
        assert_eq!(process("hello   world"), "hello world ");
        assert_eq!(process("hello ,world"), "hello,world ");
    }

    #[test]
    fn collapses_repeated_commas_from_filler_strip() {
        assert_eq!(process("um, uh, hello"), "hello ");
    }

    #[test]
    fn empty_string_passes_through() {
        assert_eq!(process(""), "");
    }

    #[test]
    fn pure_filler_input() {
        assert_eq!(process("um uh ah"), "");
    }

    #[test]
    fn preserves_oh_interjection() {
        assert_eq!(process("Oh, I didn't know that"), "Oh, I didn't know that ");
    }

    #[test]
    fn dedupes_adjacent_word_repeats() {
        assert_eq!(process("let's let's check this"), "let's check this ");
        assert_eq!(process("boat boat fish fish"), "boat fish ");
        assert_eq!(process("ha ha ha"), "ha ");
    }

    #[test]
    fn dedupe_preserves_first_casing() {
        assert_eq!(process("Let's let's go"), "Let's go ");
        assert_eq!(process("BOAT boat"), "BOAT ");
    }

    #[test]
    fn dedupe_case_insensitive_match() {
        assert_eq!(process("OpenWhisper openwhisper"), "OpenWhisper ");
    }

    #[test]
    fn dedupe_punctuation_protects_repetition() {
        // Comma between words = intentional (rhetorical emphasis).
        assert_eq!(process("really, really nice"), "really, really nice ");
        // Period between words = sentence boundary.
        assert_eq!(process("no. No problem"), "no. No problem ");
    }

    #[test]
    fn dedupe_after_substitution() {
        // "open whisper open whisper" -> subs run first, then dedupe collapses.
        assert_eq!(process("open whisper open whisper"), "OpenWhisper ");
    }

    #[test]
    fn appends_trailing_space_for_continuation() {
        // Non-empty results end in a space so a follow-up injection into
        // the same field doesn't fuse against the prior word.
        assert_eq!(process("hello"), "hello ");
        assert_eq!(process("end."), "end. ");
    }
}
