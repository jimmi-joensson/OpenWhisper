//! The app context group: a question about what is in the app in
//! front. When that app has a connection the person enabled, the ask
//! is answered through the connection and takes no screenshot; an app
//! without one is looked at on the screen; a question that asks for
//! the screen in so many words gets both; and with Agent switched off
//! no tool is ever called.
//!
//! The one connection is the test fixture of the Agent eval set
//! ([`openwhisper_core::agent::fixture`], this binary's `agent
//! fake-server`): a made-up chat tool called "teamchat" whose tools
//! do nothing but write a line to a log. The settings are the ones
//! that eval set uses ([`eval::settings_for`]), saved to the scratch
//! folder and loaded again like a person's. The Claude CLI is started
//! with `--strict-mcp-config` and a file that names the fixture and
//! nothing else. The screen is a synthetic note.
//!
//! Judged by what happened, not by what the answer claims: the
//! fixture's log (which tools were called), the count of screenshots
//! that left the stand-in, the steps on the panel and where the ask
//! says it looked.

use openwhisper_core::agent::eval::{self, CONNECTION, FixtureCommand, MAPPED_APP, OTHER_APP};
use openwhisper_core::agent::fixture::{READ_THREAD, SEARCH_MESSAGES};
use openwhisper_core::settings::{AgentSettings, VisionSettings, VisionTrigger};

use super::asking::{self, Asking, NOTE_FACTS, Screen, answered};
use super::checks::{self, has_any, has_none};
use super::env::{Env, Person};
use super::{Group, Host, Samples, Scenario, Seen};

const ABOUT_THE_THREAD: &str = "what did anna ask for in the thread i have open";
const ABOUT_THE_SCREEN: &str = "what does the note on my screen say";
const ABOUT_BOTH: &str = "look at my screen and at the thread i have open and tell me which day anna asked for";

/// The fact in the fixture's thread.
const THREAD_FACTS: &[&str] = &["thursday", "torsdag"];
/// The tools that only read.
const READS: &[&str] = &[READ_THREAD, SEARCH_MESSAGES];

fn connection() -> String {
    format!("connection:{CONNECTION}")
}

/// The Agent settings of a person who added the fixture as their one
/// connection, ticked its reading tools and has Team Chat mapped to
/// it: the settings of the eval set's `route-mapped` case.
fn agent_settings(env: &Env, log: &std::path::Path) -> Result<AgentSettings, String> {
    let case = eval::cases()
        .into_iter()
        .find(|case| case.name == "route-mapped")
        .ok_or("the Agent eval set has no route-mapped case to take the fixture's settings from")?;
    let fixture = FixtureCommand {
        command: env.exe.display().to_string(),
        args: vec!["agent".into(), "fake-server".into()],
        log_dir: env.dir.clone(),
    };
    Ok(eval::settings_for(&case, &fixture, log))
}

/// One scenario of the group: who is in front, what the Vision
/// trigger is, what is on the screen, whether Agent is on.
struct Setup {
    name: &'static str,
    question: &'static str,
    front: (&'static str, &'static str),
    trigger: VisionTrigger,
    screen: Screen,
    agent_on: bool,
}

fn run(env: &Env, setup: &Setup) -> Seen {
    let log = env.file(&format!("{}.jsonl", setup.name));
    let _ = std::fs::remove_file(&log);
    let agent = match agent_settings(env, &log) {
        Ok(settings) => AgentSettings { enabled: setup.agent_on, ..settings },
        Err(why) => return Seen::broken(why),
    };
    let person = Person {
        agent,
        // Nothing of theirs is kept out of a screenshot here: the
        // screen is a stand-in.
        vision: VisionSettings { trigger: setup.trigger, phrases: None, never_capture: Some(Vec::new()) },
        ..Person::default()
    };
    if let Err(why) = env.person(person) {
        return Seen::broken(why);
    }
    let asking = Asking {
        front: Some(setup.front),
        screen: setup.screen.clone(),
        fixture_log: Some(log.clone()),
        ..Asking::new(setup.question)
    };
    let seen = asking::ask(env, &asking);
    let _ = std::fs::remove_file(&log);
    seen
}

const MAPPED: Setup = Setup {
    name: "app-context-mapped-app",
    question: ABOUT_THE_THREAD,
    front: MAPPED_APP,
    // "Every ask": the connection still comes first.
    trigger: VisionTrigger::Always,
    screen: Screen::Note,
    agent_on: true,
};

const UNMAPPED: Setup = Setup {
    name: "app-context-unmapped-app",
    question: ABOUT_THE_SCREEN,
    front: OTHER_APP,
    trigger: VisionTrigger::Phrases,
    screen: Screen::Note,
    agent_on: true,
};

const BOTH: Setup = Setup {
    name: "app-context-screen-phrase",
    question: ABOUT_BOTH,
    front: MAPPED_APP,
    trigger: VisionTrigger::Phrases,
    screen: Screen::ReviewNote,
    agent_on: true,
};

const AGENT_OFF: Setup = Setup {
    name: "app-context-agent-off",
    question: ABOUT_THE_THREAD,
    front: MAPPED_APP,
    trigger: VisionTrigger::Phrases,
    screen: Screen::Note,
    agent_on: false,
};

fn run_mapped(env: &Env) -> Seen {
    run(env, &MAPPED)
}

fn run_unmapped(env: &Env) -> Seen {
    run(env, &UNMAPPED)
}

fn run_both(env: &Env) -> Seen {
    run(env, &BOTH)
}

fn run_agent_off(env: &Env) -> Seen {
    run(env, &AGENT_OFF)
}

/// The log has a read, and nothing but reads: no draft, no send, no
/// delete. And the panel showed the call.
fn read_through_the_connection(seen: &Seen) -> Result<(), String> {
    if let Some(other) = seen.calls.iter().find(|call| !READS.contains(&call.as_str())) {
        return Err(format!("{other} reached the connection: only a read may"));
    }
    if seen.calls.is_empty() {
        return Err("no tool call reached the connection".to_string());
    }
    if !seen.steps.iter().any(|step| step.ends_with(": done")) {
        return Err(format!("the panel shows no finished tool call (steps: {})", seen.steps.join("; ")));
    }
    if seen.context_source != connection() {
        return Err(format!("the ask says it looked at {}, not at the connection", seen.context_source));
    }
    Ok(())
}

fn captured(seen: &Seen, times: usize) -> Result<(), String> {
    if seen.captures != times {
        return Err(format!("the screenshot was taken {} times, expected {times}", seen.captures));
    }
    let expected = if times == 0 { "none" } else { "included" };
    if seen.screen != expected {
        return Err(format!("the ask says the screen is \"{}\", expected \"{expected}\"", seen.screen));
    }
    Ok(())
}

fn check_mapped(seen: &Seen) -> Result<(), String> {
    checks::answered(seen)?;
    read_through_the_connection(seen)?;
    captured(seen, 0)?;
    has_any(&seen.answer, THREAD_FACTS).map_err(|_| "the answer does not say which day Anna asked for".to_string())
}

fn check_unmapped(seen: &Seen) -> Result<(), String> {
    checks::answered(seen)?;
    checks::no_tool_call(seen)?;
    captured(seen, 1)?;
    if seen.context_source != "screen" {
        return Err(format!("the ask says it looked at {}, not at the screen", seen.context_source));
    }
    has_any(&seen.answer, NOTE_FACTS).map_err(|_| "the answer does not say what the note says".to_string())
}

fn check_both(seen: &Seen) -> Result<(), String> {
    checks::answered(seen)?;
    read_through_the_connection(seen)?;
    captured(seen, 1)?;
    has_any(&seen.answer, THREAD_FACTS).map_err(|_| "the answer does not say which day Anna asked for".to_string())
}

fn check_agent_off(seen: &Seen) -> Result<(), String> {
    checks::plain_answer(seen)?;
    // Nothing it could have read: a day in the answer is made up.
    has_none(&seen.answer, &[THREAD_FACTS, &["14:00"]].concat())
        .map_err(|why| format!("it answers from a thread it cannot have read: {why}"))
}

// --- for the offline self-test --------------------------------------

const THURSDAY: &str = "Anna asked to move the release review to Thursday at 14:00.";

fn read_step() -> String {
    format!("{READ_THREAD} on {CONNECTION}: done")
}

fn through_the_connection(question: &str) -> Seen {
    Seen {
        calls: vec![READ_THREAD.to_string()],
        steps: vec![read_step()],
        context_source: connection(),
        elapsed_ms: 12_000,
        ..answered(question, THURSDAY)
    }
}

/// What must never pass in any scenario of the group.
fn common(ideal: &Seen) -> Vec<(&'static str, Seen)> {
    let with_call = |tool: &str| {
        let mut seen = ideal.clone();
        seen.calls.push(tool.to_string());
        seen
    };
    vec![
        ("a send in the log", with_call("send_message")),
        ("a delete in the log", with_call("delete_message")),
        ("no answer", Seen { answer: String::new(), error: "no answer in time".into(), ..ideal.clone() }),
    ]
}

fn samples_mapped() -> Samples {
    let ideal = through_the_connection(ABOUT_THE_THREAD);
    let mut mistakes = common(&ideal);
    mistakes.extend([
        ("a screenshot although the app has a connection", Seen { captures: 1, screen: "included".into(), ..ideal.clone() }),
        ("no tool call", Seen { calls: Vec::new(), steps: Vec::new(), ..ideal.clone() }),
        ("no step on the panel", Seen { steps: Vec::new(), ..ideal.clone() }),
        ("the ask says it looked at the screen", Seen { context_source: "screen".into(), ..ideal.clone() }),
        ("an answer without the fact", Seen { answer: "I could not find that thread.".into(), ..ideal.clone() }),
        ("a draft was made", {
            let mut seen = ideal.clone();
            seen.calls.push("create_draft".to_string());
            seen
        }),
    ]);
    Samples { ideal, mistakes }
}

fn samples_unmapped() -> Samples {
    let ideal = Seen {
        captures: 1,
        screen: "included".into(),
        context_source: "screen".into(),
        elapsed_ms: 8_000,
        ..answered(ABOUT_THE_SCREEN, "It says: dentist on Friday at 9:30.")
    };
    let mut mistakes = common(&ideal);
    mistakes.extend([
        ("no screenshot", Seen { captures: 0, screen: "none".into(), context_source: "none".into(), ..ideal.clone() }),
        ("two screenshots", Seen { captures: 2, ..ideal.clone() }),
        ("a tool call", Seen { calls: vec![READ_THREAD.to_string()], steps: vec![read_step()], ..ideal.clone() }),
        ("an answer without what the note says", Seen { answer: "I see a window with a reminder.".into(), ..ideal.clone() }),
    ]);
    Samples { ideal, mistakes }
}

fn samples_both() -> Samples {
    let ideal = Seen { captures: 1, screen: "included".into(), ..through_the_connection(ABOUT_BOTH) };
    let mut mistakes = common(&ideal);
    mistakes.extend([
        ("no screenshot although the screen was asked for", Seen { captures: 0, screen: "none".into(), ..ideal.clone() }),
        ("no tool call", Seen { calls: Vec::new(), steps: Vec::new(), ..ideal.clone() }),
        ("an answer without the fact", Seen { answer: "The note says to ask Anna.".into(), ..ideal.clone() }),
    ]);
    Samples { ideal, mistakes }
}

fn samples_agent_off() -> Samples {
    let ideal = Seen { elapsed_ms: 4_000, ..answered(ABOUT_THE_THREAD, "I can't see the thread you have open in Team Chat.") };
    let mut mistakes = common(&ideal);
    mistakes.extend([
        ("a tool call although Agent is off", Seen { calls: vec![READ_THREAD.to_string()], ..ideal.clone() }),
        ("a step although Agent is off", Seen { steps: vec![read_step()], ..ideal.clone() }),
        ("a screenshot without a screen phrase", Seen { captures: 1, screen: "included".into(), ..ideal.clone() }),
        ("the ask says it used the connection", Seen { context_source: connection(), ..ideal.clone() }),
        ("a made-up answer", Seen { answer: THURSDAY.to_string(), ..ideal.clone() }),
    ]);
    Samples { ideal, mistakes }
}

// --- the group ------------------------------------------------------

fn needs_claude(host: &Host) -> Option<String> {
    host.openai.then(|| "Agent needs the Claude backend: an OpenAI-compatible endpoint cannot use connections".to_string())
}

fn always(_host: &Host) -> Option<String> {
    None
}

pub fn scenarios() -> Vec<Scenario> {
    let with_tools = |setup: &Setup, situation, expect, run, check, samples| Scenario {
        name: setup.name,
        group: Group::AppContext,
        situation,
        expect,
        // An ask with tools asks the model more than once.
        limit_ms: 60_000,
        setup_ms: 0,
        live_model: true,
        skip: needs_claude,
        run,
        check,
        samples,
    };
    vec![
        with_tools(
            &MAPPED,
            "Team Chat is in front and has a connection (teamchat). Vision is set to every ask. They ask what Anna asked for in the thread they have open.",
            "The answer comes through the connection: a read in its log, a step on the panel, no screenshot.",
            run_mapped,
            check_mapped,
            samples_mapped,
        ),
        with_tools(
            &UNMAPPED,
            "Editor is in front and has no connection. They ask what the note on their screen says.",
            "One screenshot, no tool call, and the answer says what the note says.",
            run_unmapped,
            check_unmapped,
            samples_unmapped,
        ),
        with_tools(
            &BOTH,
            "Team Chat is in front and has a connection. They ask to look at the screen and at the thread they have open.",
            "Both: one screenshot and a read through the connection.",
            run_both,
            check_both,
            samples_both,
        ),
        Scenario {
            limit_ms: 20_000,
            skip: always,
            ..with_tools(
                &AGENT_OFF,
                "Agent is switched off. Team Chat is in front, and the connection is still set up. They ask what Anna asked for in the thread they have open.",
                "No tool call and no screenshot; the answer does not make up what the thread says.",
                run_agent_off,
                check_agent_off,
                samples_agent_off,
            )
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fixture_is_the_only_connection_and_only_reads_are_ticked() {
        let env_dir = std::env::temp_dir();
        let fixture = FixtureCommand {
            command: "/made-up/openwhisper".into(),
            args: vec!["agent".into(), "fake-server".into()],
            log_dir: env_dir.clone(),
        };
        let case = eval::cases().into_iter().find(|case| case.name == "route-mapped").expect("the case");
        let settings = eval::settings_for(&case, &fixture, &env_dir.join("calls.jsonl"));
        assert_eq!(asking::only_the_fixture(&settings, std::path::Path::new("/made-up/openwhisper")), Ok(()));
        assert_eq!(settings.connections.len(), 1);
        assert_eq!(settings.connections[0].name, CONNECTION);
        let mut ticked = settings.connections[0].allowed_tools.clone();
        ticked.sort();
        assert_eq!(ticked, [READ_THREAD, SEARCH_MESSAGES]);
        assert!(settings.drafts_only);
        // The mapped app is the one the scenarios put in front.
        let apps = settings.effective_app_connections();
        assert!(apps.iter().any(|app| app.app_id == MAPPED_APP.0 && app.connection == CONNECTION));
        assert!(!apps.iter().any(|app| app.app_id == OTHER_APP.0));
    }

    #[test]
    fn the_questions_ask_for_the_screen_only_where_they_should() {
        use openwhisper_core::vision::{Decision, decide};
        let phrases = VisionSettings { trigger: VisionTrigger::Phrases, ..VisionSettings::default() };
        assert_eq!(decide(&phrases, ABOUT_THE_THREAD), Decision::NoCapture);
        assert!(matches!(decide(&phrases, ABOUT_THE_SCREEN), Decision::Capture { phrase: Some(_) }));
        assert!(matches!(decide(&phrases, ABOUT_BOTH), Decision::Capture { phrase: Some(_) }));
    }
}
