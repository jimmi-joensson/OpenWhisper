//! When an ask includes the screen: the decision, made on the machine
//! from the question's text and the settings, before anything is sent
//! anywhere.
//!
//! A phrase matches when its words appear in the question in order,
//! next to each other, as whole words. Case, punctuation and repeated
//! space do not matter, and an apostrophe is dropped, so "what's on my
//! screen", "Whats on my screen?" and "what’s on my screen" are the
//! same words. "on my screen" matches "what is on my screen?" and does
//! not match "onscreen" or "on my screens".

use crate::settings::{VisionSettings, VisionTrigger};

/// What the settings say about one question.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    /// No screenshot for this ask.
    NoCapture,
    /// Take one now and send it with the question. `phrase` is the
    /// phrase that matched; `None` when every ask carries the screen.
    Capture { phrase: Option<String> },
    /// Send the question as text and let the model say whether it
    /// needs the screen ([`VisionTrigger::Smart`]).
    AskModel,
}

/// The words of `text`: lower case, apostrophes dropped, everything
/// that is not a letter or a digit is a gap.
fn words(text: &str) -> Vec<String> {
    let mut flat = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\'' | '\u{2019}' | '\u{2018}' | '\u{02BC}' | '`' | '\u{00B4}' => {}
            c if c.is_alphanumeric() => flat.extend(c.to_lowercase()),
            _ => flat.push(' '),
        }
    }
    flat.split_whitespace().map(str::to_string).collect()
}

/// The first of `phrases` that `transcript` contains (see the module
/// docs for what counts), as it is written in the list. A phrase
/// without a single word in it never matches.
pub fn matched_phrase(phrases: &[String], transcript: &str) -> Option<String> {
    let said = words(transcript);
    phrases
        .iter()
        .find(|phrase| {
            let wanted = words(phrase);
            !wanted.is_empty() && said.windows(wanted.len()).any(|window| window == wanted.as_slice())
        })
        .cloned()
}

/// Whether the ask with this question includes the screen.
pub fn decide(settings: &VisionSettings, transcript: &str) -> Decision {
    let matched = || matched_phrase(&settings.effective_phrases(), transcript);
    match settings.trigger {
        VisionTrigger::Off => Decision::NoCapture,
        VisionTrigger::Always => Decision::Capture { phrase: None },
        VisionTrigger::Phrases => match matched() {
            Some(phrase) => Decision::Capture { phrase: Some(phrase) },
            None => Decision::NoCapture,
        },
        VisionTrigger::Smart => match matched() {
            Some(phrase) => Decision::Capture { phrase: Some(phrase) },
            None => Decision::AskModel,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::default_phrases;

    fn hit(transcript: &str) -> Option<String> {
        matched_phrase(&default_phrases(), transcript)
    }

    fn with(trigger: VisionTrigger) -> VisionSettings {
        VisionSettings { trigger, ..VisionSettings::default() }
    }

    #[test]
    fn english_phrases_match_as_whole_words_in_order() {
        for said in [
            "look at my screen",
            "Look at my screen, what does it say?",
            "can you look at the screen and tell me what is wrong",
            "what is on my screen?",
            "What's on my screen",
            "whats on my screen",
            "what\u{2019}s on my screen right now",
            "summarize what is ON   MY\tSCREEN.",
            "what does it say on the screen",
            "can you see my screen",
            "explain this screen to me",
            "take a screenshot and tell me what the dialog wants",
            "take a screen shot",
            "answer this message for me",
            "write a reply to this email",
            "what does this error mean",
            "(this error) again...",
        ] {
            assert!(hit(said).is_some(), "should match: {said}");
        }
    }

    #[test]
    fn english_near_misses_do_not_match() {
        for said in [
            "",
            "   ",
            "what is the capital of Norway",
            "is that onscreen keyboard any good",
            "how many screens can I connect",
            "what is on my screens",
            "put it on my big screen tv",
            "my screen is on",
            "on screen my",
            "screen on my",
            "how do screenshots work",
            "this is an error I think",
            "send this to the message board",
            "these messages are old",
            "this e-mail",
            "an error like this",
            "look at my schedule",
            "on my, well, the other screen",
        ] {
            assert_eq!(hit(said), None, "should not match: {said}");
        }
    }

    #[test]
    fn danish_phrases_match_with_their_letters_in_either_case() {
        for said in [
            "se på min skærm",
            "Kig på min skærm og sig hvad der står",
            "kan du se på skærmen",
            "kig lige på skærmen",
            "hvad står der på min skærm?",
            "HVAD STÅR DER PÅ SKÆRMEN",
            "tag et skærmbillede",
            "se på skærmbilledet",
            "svar på denne besked",
            "skriv et svar til denne mail",
            "hvad betyder denne fejl",
        ] {
            assert!(hit(said).is_some(), "should match: {said}");
        }
    }

    #[test]
    fn danish_near_misses_do_not_match() {
        for said in [
            "hvad er hovedstaden i Norge",
            "hvor mange skærme har jeg",
            "på min skærms kant",
            "skærmbilleder fylder meget",
            "pa min skaerm",
            "min skærm er i stykker",
            "disse beskeder er gamle",
            "denne gode besked",
            "fejl i denne",
            "skærmen på",
        ] {
            assert_eq!(hit(said), None, "should not match: {said}");
        }
    }

    #[test]
    fn the_phrase_comes_back_as_it_is_in_the_list_and_the_first_one_wins() {
        let phrases: Vec<String> = ["Regn Det Ud", "my screen", "on my screen"].iter().map(|p| (*p).to_string()).collect();
        assert_eq!(matched_phrase(&phrases, "what is on my screen").as_deref(), Some("my screen"));
        assert_eq!(matched_phrase(&phrases, "kan du regn det ud").as_deref(), Some("Regn Det Ud"));
        assert_eq!(matched_phrase(&phrases, "regn ud det"), None);
        // Digits are words too, and a phrase of only punctuation is
        // no phrase.
        let odd: Vec<String> = ["...", "", "window 2"].iter().map(|p| (*p).to_string()).collect();
        assert_eq!(matched_phrase(&odd, "anything at all ..."), None);
        assert_eq!(matched_phrase(&odd, "look at window 2 please").as_deref(), Some("window 2"));
        assert_eq!(matched_phrase(&odd, "look at window 22 please"), None);
        assert_eq!(matched_phrase(&[], "on my screen"), None);
    }

    #[test]
    fn apostrophes_are_dropped_on_both_sides() {
        let phrases = vec!["what's this".to_string(), "whats that".to_string()];
        assert!(matched_phrase(&phrases, "whats this").is_some());
        assert!(matched_phrase(&phrases, "what's that").is_some());
        assert!(matched_phrase(&phrases, "what`s that").is_some());
        // A dropped apostrophe joins; it does not split.
        assert_eq!(matched_phrase(&phrases, "what s this"), None);
    }

    #[test]
    fn off_never_captures() {
        for said in ["look at my screen", "what is two plus two", ""] {
            assert_eq!(decide(&with(VisionTrigger::Off), said), Decision::NoCapture);
        }
    }

    #[test]
    fn phrases_capture_on_a_match_only() {
        let s = with(VisionTrigger::Phrases);
        assert_eq!(
            decide(&s, "what does it say on my screen"),
            Decision::Capture { phrase: Some("on my screen".to_string()) }
        );
        assert_eq!(decide(&s, "hvad betyder denne fejl"), Decision::Capture { phrase: Some("denne fejl".to_string()) });
        assert_eq!(decide(&s, "what is two plus two"), Decision::NoCapture);
        assert_eq!(decide(&s, ""), Decision::NoCapture);
    }

    #[test]
    fn smart_captures_on_a_match_and_asks_the_model_otherwise() {
        let s = with(VisionTrigger::Smart);
        assert_eq!(decide(&s, "look at my screen"), Decision::Capture { phrase: Some("look at my screen".to_string()) });
        assert_eq!(decide(&s, "why is this red"), Decision::AskModel);
    }

    #[test]
    fn always_captures_whatever_is_said() {
        let s = with(VisionTrigger::Always);
        assert_eq!(decide(&s, "what is two plus two"), Decision::Capture { phrase: None });
        assert_eq!(decide(&s, "look at my screen"), Decision::Capture { phrase: None });
    }

    #[test]
    fn the_users_list_replaces_the_built_in_one() {
        let own = VisionSettings {
            trigger: VisionTrigger::Phrases,
            phrases: Some(vec!["tjek skærmen".to_string()]),
            ..VisionSettings::default()
        };
        assert_eq!(decide(&own, "tjek skærmen lige"), Decision::Capture { phrase: Some("tjek skærmen".to_string()) });
        assert_eq!(decide(&own, "look at my screen"), Decision::NoCapture);
        // A list the person emptied never matches.
        let empty = VisionSettings { phrases: Some(Vec::new()), ..own };
        assert_eq!(decide(&empty, "tjek skærmen lige"), Decision::NoCapture);
        let smart = VisionSettings { trigger: VisionTrigger::Smart, ..empty };
        assert_eq!(decide(&smart, "look at my screen"), Decision::AskModel);
    }
}
