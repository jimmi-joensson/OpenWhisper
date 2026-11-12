//! The spoken group: a question from a voice to an answer.
//!
//! The system voice stands in for the person and a file for the
//! microphone: `say -v VOICE -o FILE.aiff "..."` speaks the question,
//! the recognizer transcribes the file through the function
//! `openwhisper transcribe` calls
//! ([`openwhisper_core::file_transcription::transcribe_file`]), and
//! the transcript goes through the rule pass and the ask like any
//! spoken question ([`asking::ask_with`]).
//!
//! macOS only: `say` is what speaks. Elsewhere, and on a Mac without
//! the voice, the scenario is skipped and says why.
//!
//! The sentences were chosen by running them: the English one comes
//! back word for word; of the Danish sentences tried, this one was
//! transcribed the same, and right, three times out of three ("Hvad
//! hedder hovedstaden i Norge?" lost its last word, "Hvor mange dage
//! er der i en uge?" lost two). The transcript is judged by its key
//! words, not as a whole string.

use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use openwhisper_core::file_transcription::{FileTranscribeOptions, transcribe_file};

use super::asking::{self, Asking, answered};
use super::checks::{has_any, not_english, plain, plain_answer};
use super::env::{Env, Person};
use super::{Group, Host, Samples, Scenario, Seen};

/// One spoken question: who speaks it, what they say, what must be in
/// the transcript, and what must be in the answer.
struct Spoken {
    voice: &'static str,
    sentence: &'static str,
    heard: &'static [&'static str],
    answer: &'static [&'static str],
    danish: bool,
}

const ENGLISH: Spoken = Spoken {
    voice: "Samantha",
    sentence: "What is the capital of Australia?",
    heard: &["capital", "australia"],
    answer: &["canberra"],
    danish: false,
};

const DANISH: Spoken = Spoken {
    voice: "Sara",
    sentence: "Hvad hedder hovedstaden i Danmark?",
    heard: &["hovedstad", "danmark"],
    answer: &["k\u{f8}benhavn"],
    danish: true,
};

/// How long the first load of the recognizer may take in a fresh
/// process. Measured: 35 s for a binary that was just built, under a
/// second from then on.
const RECOGNIZER_LOAD_MS: u64 = 120_000;

/// Whether the system has the voice `name`: a line of `say -v ?`
/// begins with it.
fn has_voice(name: &str) -> bool {
    Command::new("say")
        .args(["-v", "?"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .is_some_and(|out| {
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .any(|line| line.split("  ").next().is_some_and(|voice| voice.trim() == name))
        })
}

/// Why `spoken` cannot be spoken here; `None` when it can.
fn skip(spoken: &Spoken) -> Option<String> {
    if !cfg!(target_os = "macos") {
        return Some("needs the macOS system voice (`say`) to speak the question".to_string());
    }
    if !has_voice(spoken.voice) {
        return Some(format!(
            "the system voice {} is not installed (System Settings, Accessibility, Spoken Content, System Voice, \
Manage Voices)",
            spoken.voice
        ));
    }
    None
}

fn skip_english(_host: &Host) -> Option<String> {
    skip(&ENGLISH)
}

fn skip_danish(_host: &Host) -> Option<String> {
    skip(&DANISH)
}

fn run(env: &Env, spoken: &Spoken) -> Seen {
    if let Err(why) = env.person(Person::default()) {
        return Seen::broken(why);
    }
    // The voice speaks into a file: the recording.
    let recording = env.file(&format!("spoken-{}.aiff", spoken.voice.to_lowercase()));
    let said = Command::new("say").args(["-v", spoken.voice, "-o"]).arg(&recording).arg(spoken.sentence).output();
    match said {
        Ok(out) if out.status.success() && recording.is_file() => {}
        Ok(out) => return Seen::broken(format!("`say` failed: {}", String::from_utf8_lossy(&out.stderr).trim())),
        Err(e) => return Seen::broken(format!("`say` could not be started: {e}")),
    }
    let (ask, refine) = match env.ask_and_refine() {
        Ok(both) => both,
        Err(why) => return Seen::broken(why),
    };
    let transcribe =
        || transcribe_file(&recording, &FileTranscribeOptions::default(), &AtomicBool::new(false), &mut |_| {});
    // The app has the recognizer loaded long before anybody speaks;
    // a fresh process has not, and the first load of a new binary
    // takes half a minute. So it is loaded here, by a first pass that
    // is not timed and not used.
    let _ = transcribe();
    // From here on it is what a person waits for: the transcription,
    // the rule pass, the answer.
    let started = Instant::now();
    let heard = transcribe();
    let _ = std::fs::remove_file(&recording);
    let transcript = match heard {
        Ok(heard) => heard.text.trim().to_string(),
        Err(e) => {
            return Seen {
                error: format!("the recording could not be transcribed: {e}"),
                elapsed_ms: started.elapsed().as_millis() as u64,
                ..Seen::default()
            };
        }
    };
    if transcript.is_empty() {
        return Seen {
            error: "the recognizer heard nothing in the recording".to_string(),
            elapsed_ms: started.elapsed().as_millis() as u64,
            ..Seen::default()
        };
    }
    let seen = asking::ask_with(env, &Asking::new(&transcript), &ask, &refine, started);
    Seen { transcript, ..seen }
}

fn run_english(env: &Env) -> Seen {
    run(env, &ENGLISH)
}

fn run_danish(env: &Env) -> Seen {
    run(env, &DANISH)
}

fn check(seen: &Seen, spoken: &Spoken) -> Result<(), String> {
    // What was heard first: an answer to a misheard question says
    // nothing about the ask.
    if !seen.transcript.is_empty() || seen.error.is_empty() {
        let heard = plain(&seen.transcript);
        if let Some(missing) = spoken.heard.iter().find(|word| !heard.contains(&plain(word))) {
            return Err(format!("the recognizer heard \"{}\", without \"{missing}\"", seen.transcript));
        }
    }
    plain_answer(seen)?;
    has_any(&seen.answer, spoken.answer).map_err(|_| format!("the answer does not say {}", spoken.answer.join(" or ")))?;
    if spoken.danish {
        not_english(&seen.answer)?;
    }
    Ok(())
}

fn check_english(seen: &Seen) -> Result<(), String> {
    check(seen, &ENGLISH)
}

fn check_danish(seen: &Seen) -> Result<(), String> {
    check(seen, &DANISH)
}

fn samples(spoken: &Spoken, transcript: &str, answer: &str, misheard: &str, wrong: &str) -> Samples {
    let ideal = Seen { transcript: transcript.to_string(), elapsed_ms: 4_000, ..answered(transcript, answer) };
    let mut mistakes = vec![
        ("the question was misheard", Seen { transcript: misheard.to_string(), question: misheard.to_string(), ..ideal.clone() }),
        ("nothing was heard", Seen { transcript: String::new(), ..ideal.clone() }),
        ("the wrong answer", Seen { answer: wrong.to_string(), ..ideal.clone() }),
        (
            "no answer",
            Seen { answer: String::new(), error: "Claude Code is not installed on this computer.".into(), ..ideal.clone() },
        ),
        (
            "the recording could not be transcribed",
            Seen {
                transcript: String::new(),
                answer: String::new(),
                error: "the recording could not be transcribed: no audio".into(),
                ..ideal.clone()
            },
        ),
        ("a screenshot was taken", Seen { captures: 1, ..ideal.clone() }),
    ];
    if spoken.danish {
        mistakes.push(("answered in English", Seen { answer: "The capital of Denmark is K\u{f8}benhavn.".into(), ..ideal.clone() }));
    }
    Samples { ideal, mistakes }
}

fn samples_english() -> Samples {
    samples(
        &ENGLISH,
        "What is the capital of Australia?",
        "The capital of Australia is Canberra.",
        "What is the capital of Austria?",
        "The capital of Australia is Sydney.",
    )
}

fn samples_danish() -> Samples {
    samples(
        &DANISH,
        "Hvad hedder hovedstaden i Danmark?",
        "Hovedstaden i Danmark hedder K\u{f8}benhavn.",
        "Hvad hedder hovedstaden i n\u{e5}?",
        "Hovedstaden i Danmark hedder Aarhus.",
    )
}

pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "spoken-english",
            group: Group::Spoken,
            situation: "They say, in English: \"What is the capital of Australia?\" (the system voice Samantha).",
            expect: "The recognizer hears the question, and the answer says Canberra.",
            limit_ms: 30_000,
            setup_ms: RECOGNIZER_LOAD_MS,
            live_model: true,
            skip: skip_english,
            run: run_english,
            check: check_english,
            samples: samples_english,
        },
        Scenario {
            name: "spoken-danish",
            group: Group::Spoken,
            situation: "They say, in Danish: \"Hvad hedder hovedstaden i Danmark?\" (the system voice Sara).",
            expect: "The recognizer hears the question, and the answer says K\u{f8}benhavn, in Danish.",
            limit_ms: 30_000,
            setup_ms: RECOGNIZER_LOAD_MS,
            live_model: true,
            skip: skip_danish,
            run: run_danish,
            check: check_danish,
            samples: samples_danish,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_transcript_is_judged_by_its_key_words() {
        let heard = |transcript: &str| Seen {
            transcript: transcript.to_string(),
            ..answered(transcript, "The capital of Australia is Canberra.")
        };
        // Case, punctuation and a filler do not matter.
        for fine in ["What is the capital of Australia?", "what's the capital of australia", "So what is the capital of Australia"] {
            assert_eq!(check_english(&heard(fine)), Ok(()), "{fine}");
        }
        let problem = check_english(&heard("What is the capital of Austria?")).unwrap_err();
        assert!(problem.contains("without \"australia\""), "{problem}");
    }

    #[test]
    fn off_the_mac_the_group_is_skipped_with_a_reason() {
        let reason = skip(&ENGLISH);
        if cfg!(target_os = "macos") {
            // On a Mac it depends on the voice, and a missing one is named.
            assert!(reason.is_none_or(|why| why.contains("Samantha")));
        } else {
            assert!(reason.is_some_and(|why| why.contains("macOS system voice")));
        }
    }
}
