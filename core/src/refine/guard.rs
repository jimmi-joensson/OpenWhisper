//! Cheap checks on what the model returned, before it replaces the
//! transcript.
//!
//! The prompt tells the model to clean and not to obey, answer,
//! summarize or translate. This is the second line of defense for the
//! times it does so anyway: text that is far longer or shorter than
//! what was said, or that shares too few words with it, is rejected
//! and the unrefined transcript is inserted instead.
//!
//! The checks are deliberately crude. They never judge whether the
//! cleanup is good, only whether it is still recognisably the same
//! text.
//!
//! One exception: when the speaker gave Refine an instruction in the
//! dictation ("make that more formal", "write this in English") and
//! the model declared that it followed one, the text is supposed to
//! differ. Then only [`check_instructed`] applies.

use std::collections::HashSet;

/// Output may be this many times longer than the input (in chars),
/// plus [`GROWTH_SLACK_CHARS`]. Punctuation and fixed words add a
/// little; an answer or an email adds a lot.
const MAX_GROWTH: f64 = 1.5;
const GROWTH_SLACK_CHARS: usize = 30;

/// From this many input words on, the output must keep at least half
/// of them (by count). Removing fillers shrinks text a bit; a summary
/// shrinks it a lot.
const SHRINK_MIN_INPUT_WORDS: usize = 8;

/// From this many output words on, at most this share of them may be
/// words that were not in the input.
const DRIFT_MIN_OUTPUT_WORDS: usize = 5;
const MAX_NOVEL_SHARE: f64 = 0.5;

/// `Ok` when `output` is a plausible cleanup of `input`, otherwise a
/// short reason for the UI and logs. `drift_guard` turns the
/// shared-words check off (for users whose instructions ask for heavy
/// rewriting on purpose); the length checks always run.
pub fn check(input: &str, output: &str, drift_guard: bool) -> Result<(), String> {
    let input = input.trim();
    let output = output.trim();
    if output.is_empty() {
        return Err("the model returned no text".into());
    }

    let in_chars = input.chars().count();
    let out_chars = output.chars().count();
    let limit = (in_chars as f64 * MAX_GROWTH) as usize + GROWTH_SLACK_CHARS;
    if out_chars > limit {
        return Err(format!(
            "the model returned much more text than was said ({out_chars} vs {in_chars} characters)"
        ));
    }

    let in_words = words(input);
    let out_words = words(output);
    if in_words.len() >= SHRINK_MIN_INPUT_WORDS && out_words.len() * 2 < in_words.len() {
        return Err(format!(
            "the model dropped more than half of what was said ({} of {} words left)",
            out_words.len(),
            in_words.len()
        ));
    }

    if drift_guard && out_words.len() >= DRIFT_MIN_OUTPUT_WORDS {
        let said: HashSet<&str> = in_words.iter().map(String::as_str).collect();
        let novel = out_words
            .iter()
            .filter(|w| !said.contains(w.as_str()))
            .count();
        if novel as f64 > out_words.len() as f64 * MAX_NOVEL_SHARE {
            return Err(format!(
                "the model changed too many words ({novel} of {} were not in what was said)",
                out_words.len()
            ));
        }
    }
    Ok(())
}

/// With a declared spoken instruction ("make it formal", "in English",
/// "as a bullet list") the text is allowed to differ from what was
/// said, so the word and shrink checks do not apply. What still holds:
/// there must be text, and it may not balloon. Rewriting, translating
/// or reformatting what was said stays within a few times its length;
/// an essay does not.
const INSTRUCTED_MAX_GROWTH: f64 = 3.0;
const INSTRUCTED_SLACK_CHARS: usize = 200;

/// The check for output where the model declared it carried out a
/// spoken instruction (see [`super::prompt::split_instruction`]).
pub fn check_instructed(input: &str, output: &str) -> Result<(), String> {
    let output = output.trim();
    if output.is_empty() {
        return Err("the model returned no text".into());
    }
    let in_chars = input.trim().chars().count();
    let out_chars = output.chars().count();
    let limit = (in_chars as f64 * INSTRUCTED_MAX_GROWTH) as usize + INSTRUCTED_SLACK_CHARS;
    if out_chars > limit {
        return Err(format!(
            "the model returned much more text than was said ({out_chars} vs {in_chars} characters)"
        ));
    }
    Ok(())
}

/// Lowercased words, punctuation stripped. Apostrophes and hyphens
/// inside a word are dropped so "don't" and "dont", "e-mail" and
/// "email" compare equal.
fn words(text: &str) -> Vec<String> {
    text.split(|c: char| c.is_whitespace())
        .map(|w| {
            w.chars()
                .filter(|c| c.is_alphanumeric())
                .flat_map(char::to_lowercase)
                .collect::<String>()
        })
        .filter(|w| !w.is_empty())
        .collect()
}

/// Word count as the pipeline sees it (for the short-utterance skip).
pub fn word_count(text: &str) -> usize {
    words(text).len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_normal_cleanup() {
        let input = "so um i think we should move the meeting with jeppe to thursday";
        let output = "I think we should move the meeting with Jeppe to Thursday.";
        assert!(check(input, output, true).is_ok());
    }

    #[test]
    fn accepts_unchanged_text() {
        let text = "Det er fint, vi tager den i morgen.";
        assert!(check(text, text, true).is_ok());
    }

    #[test]
    fn rejects_empty_output() {
        assert!(check("hello there everyone", "   ", true).is_err());
    }

    #[test]
    fn rejects_an_email_written_from_a_command() {
        let input = "write an email to Jeppe about the invoice";
        let output = "Subject: Invoice\n\nHi Jeppe,\n\nI hope you are well. I am writing about \
the invoice we discussed last week. Could you send me the latest version?\n\nBest regards";
        let err = check(input, output, true).unwrap_err();
        assert!(err.contains("much more text"), "{err}");
    }

    #[test]
    fn rejects_an_answer_to_a_question() {
        let input = "can you tell me what the capital of france is and why";
        let output = "The capital is Paris, chosen for its central location and long history.";
        let err = check(input, output, true).unwrap_err();
        assert!(err.contains("changed too many words"), "{err}");
    }

    #[test]
    fn rejects_a_translation() {
        let input = "jeg synes vi skal flytte mødet til torsdag fordi der er release onsdag";
        let output = "I think we should move the meeting to Thursday because the release is Wednesday.";
        assert!(check(input, output, true).is_err());
    }

    #[test]
    fn rejects_a_summary() {
        let input = "so first we need to update the api then we deploy to staging and after \
that we run the smoke tests and if they pass we ship to production on friday";
        let output = "Update, deploy, test, ship Friday.";
        let err = check(input, output, true).unwrap_err();
        assert!(err.contains("dropped more than half"), "{err}");
    }

    #[test]
    fn drift_guard_off_allows_rewrites_but_not_length_blowups() {
        let input = "jeg synes vi skal flytte mødet til torsdag fordi der er release onsdag";
        let translated =
            "I think we should move the meeting to Thursday because the release is Wednesday.";
        assert!(check(input, translated, false).is_ok());
        let blown = translated.repeat(4);
        assert!(check(input, &blown, false).is_err());
    }

    #[test]
    fn short_output_is_not_drift_checked() {
        // Four words, two "new" through a mis-hearing fix. Too short
        // for a word-share check to mean anything.
        assert!(check("their going too fast", "They're going too fast.", true).is_ok());
    }

    #[test]
    fn instructed_output_may_be_reworded_translated_or_shorter() {
        let input = "hej emanoil vi rykker mødet til fredag skriv det på engelsk";
        assert!(check_instructed(input, "Hi Emanoil, we're moving the meeting to Friday.").is_ok());
        let input = "hi jeppe the invoice is ready no wait scratch that the invoice is delayed";
        assert!(check_instructed(input, "Hi Jeppe, the invoice is delayed.").is_ok());
    }

    #[test]
    fn instructed_output_still_needs_text_and_a_sane_length() {
        let input = "tell jeppe the release is delayed make it more formal";
        assert!(check_instructed(input, "  ").is_err());
        let essay = "The release has been delayed. ".repeat(40);
        assert!(check_instructed(input, &essay).is_err());
    }

    #[test]
    fn word_count_ignores_punctuation() {
        assert_eq!(word_count("Hello, world - again!"), 3);
        assert_eq!(word_count("  "), 0);
    }
}
