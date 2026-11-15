//! The modes group: what Refine makes of a dictation. A mode of the
//! person's own changes the text as its instructions say; an app's
//! default mode wins over the selected one; and the built-in
//! Dictation mode leaves a clean sentence alone.
//!
//! The modes are made the way `openwhisper modes add`, `select` and
//! `default` make them ([`modes::add`], [`modes::select`],
//! [`modes::set_app_default`]) and saved to the scratch folder. A
//! dictation is then resolved and refined the way the pipeline does
//! it without a warmed backend, and the way `refine run --app` does
//! it: [`modes::resolve`] and [`refine::refine_text_in_mode`], after
//! the rule pass.
//!
//! The instructions are made up, and chosen so that their effect can
//! be read off the text: capital letters and a last word, a word in
//! front.

use std::time::Instant;

use openwhisper_core::modes;
use openwhisper_core::refine;
use openwhisper_core::settings::{BUILTIN_MODE_ID, ModesSettings, RefineBackendKind, RefineEffort, RefineSettings};
use openwhisper_core::transcript;

use super::checks::{has_any, has_none};
use super::env::{Env, Person};
use super::{Group, Host, Samples, Scenario, Seen};

const SAID: &str = "the meeting is moved to thursday at two";
const CLEAN: &str = "The meeting is moved to Thursday at two.";

const SHOUT: (&str, &str) = ("Shout", "Rewrite the text in capital letters and end it with the word AHOY.");
const NOTE: (&str, &str) = ("Note", "Put the word NOTE and a colon in front of the text. Change nothing else.");
/// The app that has a default mode. Made up.
const APP: (&str, &str) = ("com.example.editor", "Editor");

/// Refine as a person has it who uses modes: switched on, with room
/// for a cold start of the backend (a dictation in the app has the
/// backend warmed while they speak; a run here has not).
fn refine_on() -> RefineSettings {
    RefineSettings { enabled: true, timeout_ms: 20_000, ..RefineSettings::default() }
}

/// The id a mode got when it was added.
fn id_of(settings: &ModesSettings, name: &str) -> Result<String, String> {
    settings.custom.iter().find(|m| m.name == name).map(|m| m.id.clone()).ok_or(format!("the mode {name} was not added"))
}

/// The Refine settings of the run: what is saved, with the backend
/// overrides on top. A model or effort override is for the backend
/// in use, so for Claude it lands in the Refine block here (for an
/// OpenAI-compatible endpoint the ask loader already put it there).
fn refine_settings(env: &Env) -> Result<RefineSettings, String> {
    let (_, mut refine) = env.ask_and_refine()?;
    if refine.backend != RefineBackendKind::OpenaiCompatible {
        if let Some(model) = &env.overrides.model {
            refine.claude_cli.model = model.clone();
        }
        if let Some(effort) = env.overrides.effort.as_deref().and_then(RefineEffort::parse) {
            refine.claude_cli.effort = effort;
        }
    }
    Ok(refine)
}

/// Dictate `said` into `app` with the modes of `person`, and report
/// what the text became.
fn dictate(env: &Env, modes: Result<ModesSettings, String>, said: &str, app: Option<(&str, &str)>) -> Seen {
    let modes = match modes {
        Ok(modes) => modes,
        Err(why) => return Seen::broken(why),
    };
    if let Err(why) = env.person(Person { refine: refine_on(), modes, ..Person::default() }) {
        return Seen::broken(why);
    }
    let refine = match refine_settings(env) {
        Ok(refine) => refine,
        Err(why) => return Seen::broken(why),
    };
    let started = Instant::now();
    // The rule pass every dictation gets, then the mode that applies.
    let text = transcript::process(said);
    let choice = modes::resolve(&env.modes(), &refine, app);
    let outcome = refine::refine_text_in_mode(&refine, &choice, &text);
    Seen {
        question: text.trim().to_string(),
        answer: outcome.text.trim().to_string(),
        error: if outcome.status.is_fallback() {
            format!("{}: {}", outcome.status.as_str(), outcome.detail)
        } else {
            String::new()
        },
        status: outcome.status.as_str().to_string(),
        mode: outcome.mode.id,
        mode_reason: outcome.mode.reason,
        screen: "none".to_string(),
        context_source: "none".to_string(),
        elapsed_ms: started.elapsed().as_millis() as u64,
        ..Seen::default()
    }
}

fn with_shout_selected() -> Result<ModesSettings, String> {
    let added = modes::add(&ModesSettings::default(), SHOUT.0, SHOUT.1, None, None, None)?;
    modes::select(&added, &id_of(&added, SHOUT.0)?)
}

fn with_a_default_for_the_app() -> Result<ModesSettings, String> {
    let shout = with_shout_selected()?;
    let both = modes::add(&shout, NOTE.0, NOTE.1, None, None, None)?;
    modes::set_app_default(&both, APP.0, APP.1, &id_of(&both, NOTE.0)?)
}

fn run_custom(env: &Env) -> Seen {
    dictate(env, with_shout_selected(), SAID, None)
}

fn run_app_default(env: &Env) -> Seen {
    dictate(env, with_a_default_for_the_app(), SAID, Some(APP))
}

fn run_dictation(env: &Env) -> Seen {
    dictate(env, Ok(ModesSettings::default()), CLEAN, None)
}

/// The run was made in `mode`, for `reason`, and the model's text was
/// used.
fn ran_in(seen: &Seen, mode: &str, reason: &str) -> Result<(), String> {
    if !seen.error.is_empty() {
        return Err(format!("the text was not refined: {}", seen.error));
    }
    if seen.status != "refined" {
        return Err(format!("the run ended as \"{}\", not as refined", seen.status));
    }
    if seen.mode != mode || seen.mode_reason != reason {
        return Err(format!("the run was made in the mode {} ({}), expected {mode} ({reason})", seen.mode, seen.mode_reason));
    }
    Ok(())
}

/// What was said is still in the text.
fn keeps_what_was_said(seen: &Seen) -> Result<(), String> {
    for word in ["meeting", "thursday", "two"] {
        has_any(&seen.answer, &[word]).map_err(|_| format!("\"{word}\" is gone from the text: {}", seen.answer))?;
    }
    Ok(())
}

fn check_custom(seen: &Seen) -> Result<(), String> {
    ran_in(seen, "shout", "selected")?;
    keeps_what_was_said(seen)?;
    if let Some(lower) = seen.answer.chars().find(|c| c.is_lowercase()) {
        return Err(format!("the text is not in capital letters (\"{lower}\"): {}", seen.answer));
    }
    let last = seen.answer.trim_end_matches(|c: char| !c.is_alphanumeric());
    if !last.ends_with("AHOY") {
        return Err(format!("the text does not end with AHOY: {}", seen.answer));
    }
    Ok(())
}

fn check_app_default(seen: &Seen) -> Result<(), String> {
    ran_in(seen, "note", "app-default")?;
    keeps_what_was_said(seen)?;
    if !seen.answer.trim_start().to_uppercase().starts_with("NOTE:") {
        return Err(format!("the text does not begin with NOTE and a colon: {}", seen.answer));
    }
    // The selected mode's mark must not be on it.
    has_none(&seen.answer, &["ahoy"]).map_err(|_| format!("the selected mode ran, not the app's default: {}", seen.answer))
}

fn check_dictation(seen: &Seen) -> Result<(), String> {
    ran_in(seen, BUILTIN_MODE_ID, "selected")?;
    if seen.answer.trim() != CLEAN {
        return Err(format!("a clean sentence was changed: \"{}\"", seen.answer));
    }
    Ok(())
}

// --- for the offline self-test --------------------------------------

fn refined(said: &str, text: &str, mode: &str, reason: &str) -> Seen {
    Seen {
        question: said.to_string(),
        answer: text.to_string(),
        status: "refined".to_string(),
        mode: mode.to_string(),
        mode_reason: reason.to_string(),
        screen: "none".to_string(),
        context_source: "none".to_string(),
        elapsed_ms: 3_000,
        ..Seen::default()
    }
}

/// The backend did not answer: the text comes back as it was said.
fn fell_back(ideal: &Seen) -> Seen {
    Seen {
        answer: ideal.question.clone(),
        status: "timeout".to_string(),
        error: "timeout: no answer in time".to_string(),
        ..ideal.clone()
    }
}

fn saying(ideal: &Seen, text: &str) -> Seen {
    Seen { answer: text.to_string(), ..ideal.clone() }
}

fn samples_custom() -> Samples {
    let ideal = refined(SAID, "THE MEETING IS MOVED TO THURSDAY AT TWO. AHOY", "shout", "selected");
    let mistakes = vec![
        ("the text is unchanged", saying(&ideal, SAID)),
        ("capital letters without the last word", saying(&ideal, "THE MEETING IS MOVED TO THURSDAY AT TWO.")),
        ("the last word without capital letters", saying(&ideal, "The meeting is moved to Thursday at two. AHOY")),
        ("what was said is gone", saying(&ideal, "AHOY")),
        ("the backend did not answer", fell_back(&ideal)),
        ("the built-in mode ran", Seen { mode: BUILTIN_MODE_ID.to_string(), ..ideal.clone() }),
    ];
    Samples { ideal, mistakes }
}

fn samples_app_default() -> Samples {
    let ideal = refined(SAID, "NOTE: the meeting is moved to thursday at two", "note", "app-default");
    let mistakes = vec![
        ("the selected mode ran", refined(SAID, "THE MEETING IS MOVED TO THURSDAY AT TWO. AHOY", "shout", "selected")),
        ("the selected mode's text under the app's mode", saying(&ideal, "NOTE: THE MEETING IS MOVED TO THURSDAY AT TWO. AHOY")),
        ("the text is unchanged", saying(&ideal, SAID)),
        ("the mode ran as the selected one", Seen { mode_reason: "selected".to_string(), ..ideal.clone() }),
        ("the backend did not answer", fell_back(&ideal)),
    ];
    Samples { ideal, mistakes }
}

fn samples_dictation() -> Samples {
    let ideal = refined(CLEAN, CLEAN, BUILTIN_MODE_ID, "selected");
    let mistakes = vec![
        ("the sentence was reworded", saying(&ideal, "The meeting has been moved to Thursday at 2.")),
        ("the full stop is gone", saying(&ideal, "The meeting is moved to Thursday at two")),
        ("a custom mode ran", refined(CLEAN, CLEAN, "shout", "selected")),
        ("the backend did not answer", fell_back(&ideal)),
    ];
    Samples { ideal, mistakes }
}

// --- the group ------------------------------------------------------

fn always(_host: &Host) -> Option<String> {
    None
}

pub fn scenarios() -> Vec<Scenario> {
    let mode = |name, situation, expect, run, check, samples| Scenario {
        name,
        group: Group::Modes,
        situation,
        expect,
        limit_ms: 25_000,
        setup_ms: 0,
        live_model: true,
        skip: always,
        run,
        check,
        samples,
    };
    vec![
        mode(
            "modes-custom-mode",
            "They made a mode that says: capital letters, and end with the word AHOY. It is selected, and they dictate a sentence.",
            "The sentence in capital letters, ending with AHOY.",
            run_custom,
            check_custom,
            samples_custom,
        ),
        mode(
            "modes-app-default-wins",
            "That mode is selected, and the app Editor has another mode as its default (put NOTE: in front). They dictate into Editor.",
            "The text begins with NOTE: and has no AHOY: the app's default ran.",
            run_app_default,
            check_app_default,
            samples_app_default,
        ),
        mode(
            "modes-dictation-clean",
            "The built-in Dictation mode is selected. They dictate a sentence that needs no cleanup.",
            "The sentence comes back exactly as it was.",
            run_dictation,
            check_dictation,
            samples_dictation,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_modes_are_made_the_way_the_commands_make_them() {
        let shout = with_shout_selected().unwrap();
        assert_eq!(shout.active, "shout");
        let both = with_a_default_for_the_app().unwrap();
        assert_eq!(both.active, "shout");
        assert_eq!(both.custom.len(), 2);
        // A dictation into the app runs in its default, elsewhere in
        // the selected mode.
        let refine = refine_on();
        let in_app = modes::resolve(&both, &refine, Some(APP));
        assert_eq!((in_app.mode.id.as_str(), in_app.reason.as_str()), ("note", "app-default"));
        let elsewhere = modes::resolve(&both, &refine, Some(("com.example.other", "Other")));
        assert_eq!((elsewhere.mode.id.as_str(), elsewhere.reason.as_str()), ("shout", "selected"));
        let builtin = modes::resolve(&ModesSettings::default(), &refine, None);
        assert_eq!(builtin.mode.id, BUILTIN_MODE_ID);
    }

    #[test]
    fn the_rule_pass_leaves_the_clean_sentence_alone() {
        assert_eq!(transcript::process(CLEAN).trim(), CLEAN);
    }
}
