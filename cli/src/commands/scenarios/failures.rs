//! The failures group: what a person sees when something is missing.
//! Each scenario must end with a plain message, with no answer where
//! none can be, and within its time limit. A hang is a failure.
//!
//! - The Claude CLI is not there, or is there and signed out (a
//!   shell script stands in for it and prints what the real one
//!   prints).
//! - The OpenAI-compatible endpoint cannot be reached (a loopback
//!   port nothing listens on).
//! - The model does not answer in time (a stand-in that takes the
//!   question and stays silent), and the process is gone afterwards.
//! - The person cancels in the middle of an answer of the real
//!   model: the ask ends at once and the CLI's process is gone. The
//!   process id comes from a wrapper script that writes it down and
//!   then becomes the real CLI (`exec`), so the process the app
//!   starts and stops is the real one.
//! - A screenshot is wanted and cannot be taken: no permission, a
//!   protected app in front, no display. The platform is made up
//!   ([`asking::Screen`]); the ask goes on as text and says why.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use openwhisper_core::refine::claude_cli::find_binary;
use openwhisper_core::settings::{
    ASK_TIMEOUT_MIN_MS, NeverCaptureApp, RefineBackendKind, VisionSettings, VisionTrigger,
};

use super::asking::{self, Asking, NOTE_FACTS, Screen, answered};
use super::checks::{self, failed_with, has_any, has_none, no_capture, no_tool_call, plain_message};
use super::env::{Env, Person, needs_a_shell};
use super::{Group, Host, Samples, Scenario, Seen};

const QUESTION: &str = "what is the capital of norway";
const LONG_QUESTION: &str = "write a story of about three hundred words about a lighthouse keeper and her cat";
const ABOUT_THE_SCREEN: &str = "what does the note on my screen say";

/// The app the person keeps out of every screenshot. Made up.
const VAULT: (&str, &str) = ("com.example.vault", "Vault");
const EDITOR: (&str, &str) = ("com.example.editor", "Editor");

/// How long after the cancel the ask may take to end.
const CANCEL_LIMIT_MS: u64 = 3_000;

/// A stand-in for a Claude CLI that is installed and signed out: it
/// says what the real one says to `auth status` and to a question.
const SIGNED_OUT: &str = r#"#!/bin/sh
if [ "$1" = "auth" ]; then echo '{"loggedIn": false}'; exit 1; fi
if [ "$1" = "--version" ]; then echo '9.9.9 (stand-in)'; exit 0; fi
read -r line
echo '{"type":"result","subtype":"success","is_error":true,"result":"Invalid API key · Please run /login"}'
"#;

/// A stand-in for a Claude CLI that takes the question and never
/// answers. It writes its process id down first, and stays the same
/// process while it is silent.
const SILENT: &str = r#"#!/bin/sh
if [ "$1" = "auth" ]; then echo '{"loggedIn": true, "authMethod": "claude.ai", "subscriptionType": "max"}'; exit 0; fi
if [ "$1" = "--version" ]; then echo '9.9.9 (stand-in)'; exit 0; fi
echo $$ > "$(dirname "$0")/silent.pid"
read -r line
exec sleep 120
"#;

/// `text` inside single quotes for a shell.
fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// A wrapper around the real CLI at `real`: it writes its process id
/// to `pid_file` and then becomes the real CLI, with the same
/// arguments, input and output.
fn wrapper(real: &Path, pid_file: &Path) -> String {
    let real = quoted(&real.display().to_string());
    format!(
        "#!/bin/sh\ncase \"$1\" in auth|--version) exec {real} \"$@\" ;; esac\necho $$ > {}\nexec {real} \"$@\"\n",
        quoted(&pid_file.display().to_string())
    )
}

/// Whether the process `pid` is gone, waiting up to `within` for it
/// to go. `None` when that cannot be told here.
fn gone(pid: u32, within: Duration) -> Option<bool> {
    if !cfg!(unix) {
        return None;
    }
    let started = Instant::now();
    loop {
        // Signal 0 reaches a process that is there and does nothing
        // to it.
        let there = Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .ok()?
            .success();
        if !there {
            return Some(true);
        }
        if started.elapsed() >= within {
            return Some(false);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// The process whose id is in `pid_file` is gone. `Some(false)` also
/// when the file is not there: the process was never seen.
fn process_gone(pid_file: &Path) -> Option<bool> {
    let pid = std::fs::read_to_string(pid_file).ok().and_then(|text| text.trim().parse::<u32>().ok());
    let _ = std::fs::remove_file(pid_file);
    match pid {
        Some(pid) => gone(pid, Duration::from_secs(2)),
        None => Some(false),
    }
}

/// Ask [`QUESTION`] with the settings of `person` exactly as saved.
fn asked_as_saved(env: &Env, person: Person) -> Seen {
    if let Err(why) = env.person(person) {
        return Seen::broken(why);
    }
    asking::ask(env, &Asking { as_saved: true, ..Asking::new(QUESTION) })
}

// --- the backend is not there ---------------------------------------

fn run_cli_missing(env: &Env) -> Seen {
    let mut person = Person::default();
    person.refine.claude_cli.path = Some(env.file("bin").join("claude-is-not-here").display().to_string());
    asked_as_saved(env, person)
}

fn check_cli_missing(seen: &Seen) -> Result<(), String> {
    failed_with(seen, &["not installed"])?;
    nothing_else_happened(seen)
}

fn run_signed_out(env: &Env) -> Seen {
    let script = match env.script("claude-signed-out", SIGNED_OUT) {
        Ok(path) => path,
        Err(why) => return Seen::broken(why),
    };
    let mut person = Person::default();
    person.refine.claude_cli.path = Some(script.display().to_string());
    asked_as_saved(env, person)
}

fn check_signed_out(seen: &Seen) -> Result<(), String> {
    failed_with(seen, &["not signed in", "sign in", "signed out"])?;
    nothing_else_happened(seen)
}

fn run_unreachable(env: &Env) -> Seen {
    // A port of this computer that nothing listens on: taken for a
    // moment, then closed again.
    let port = match std::net::TcpListener::bind(("127.0.0.1", 0)).and_then(|listener| listener.local_addr()) {
        Ok(address) => address.port(),
        Err(e) => return Seen::broken(format!("no loopback port to point at: {e}")),
    };
    let mut person = Person::default();
    person.refine.backend = RefineBackendKind::OpenaiCompatible;
    person.refine.openai.base_url = format!("http://127.0.0.1:{port}/v1");
    person.refine.openai.model = "local-model".to_string();
    asked_as_saved(env, person)
}

fn check_unreachable(seen: &Seen) -> Result<(), String> {
    failed_with(seen, &["could not reach", "cannot reach", "can't reach", "not reachable", "connect"])?;
    nothing_else_happened(seen)
}

/// A failed ask did nothing on the side.
fn nothing_else_happened(seen: &Seen) -> Result<(), String> {
    no_tool_call(seen)?;
    no_capture(seen)
}

// --- no answer in time, and a cancel --------------------------------

fn run_timeout(env: &Env) -> Seen {
    let script = match env.script("claude-silent", SILENT) {
        Ok(path) => path,
        Err(why) => return Seen::broken(why),
    };
    let pid_file = script.with_file_name("silent.pid");
    let _ = std::fs::remove_file(&pid_file);
    let mut person = Person::default();
    person.refine.claude_cli.path = Some(script.display().to_string());
    // The shortest wait a person can set.
    person.ask.timeout_ms = ASK_TIMEOUT_MIN_MS;
    let seen = asked_as_saved(env, person);
    Seen { process_gone: process_gone(&pid_file), ..seen }
}

fn check_timeout(seen: &Seen) -> Result<(), String> {
    failed_with(seen, &["in time", "timed out", "timeout", "too long"])?;
    nothing_else_happened(seen)?;
    // It waited as long as the person set, not shorter.
    if seen.elapsed_ms + 500 < ASK_TIMEOUT_MIN_MS {
        return Err(format!("it gave up after {} ms, before the {} ms the person set", seen.elapsed_ms, ASK_TIMEOUT_MIN_MS));
    }
    stopped_its_process(seen)
}

fn stopped_its_process(seen: &Seen) -> Result<(), String> {
    match seen.process_gone {
        Some(true) => Ok(()),
        Some(false) => Err("the backend's process is still running, or was never started".to_string()),
        None => Err("it could not be told whether the backend's process is gone".to_string()),
    }
}

fn run_cancel(env: &Env) -> Seen {
    if let Err(why) = env.person(Person::default()) {
        return Seen::broken(why);
    }
    let (ask, mut refine) = match env.ask_and_refine() {
        Ok(both) => both,
        Err(why) => return Seen::broken(why),
    };
    // The real CLI, behind a wrapper that only writes down which
    // process it is.
    let Some(real) = find_binary(&refine.claude_cli) else {
        return Seen::broken("Claude Code is not installed, so there is no answer to cancel");
    };
    let pid_file = env.file("bin").join("cancel.pid");
    let _ = std::fs::remove_file(&pid_file);
    let script = match env.script("claude-with-pid", &wrapper(&real, &pid_file)) {
        Ok(path) => path,
        Err(why) => return Seen::broken(why),
    };
    refine.claude_cli.path = Some(script.display().to_string());
    let asking = Asking { cancel_after_first_text: Some(Duration::from_millis(300)), ..Asking::new(LONG_QUESTION) };
    let seen = asking::ask_with(env, &asking, &ask, &refine, Instant::now());
    Seen { process_gone: process_gone(&pid_file), ..seen }
}

fn check_cancel(seen: &Seen) -> Result<(), String> {
    let Some(after_cancel) = seen.cancel_to_end_ms else {
        return Err(if seen.error.is_empty() {
            "the answer never started, so nothing was cancelled".to_string()
        } else {
            format!("the answer never started, so nothing was cancelled: {}", seen.error)
        });
    };
    if !seen.answer.is_empty() {
        return Err("an answer came back although the ask was cancelled".to_string());
    }
    failed_with(seen, &["cancel"])?;
    if after_cancel > CANCEL_LIMIT_MS {
        return Err(format!("the ask went on for {after_cancel} ms after the cancel"));
    }
    nothing_else_happened(seen)?;
    stopped_its_process(seen)
}

// --- no screenshot --------------------------------------------------

/// Vision as a person has it who keeps one app out of every
/// screenshot, and a question that asks for the screen.
fn without_a_screenshot(env: &Env, screen: Screen, front: (&'static str, &'static str)) -> Seen {
    let person = Person {
        vision: VisionSettings {
            trigger: VisionTrigger::Phrases,
            phrases: None,
            never_capture: Some(vec![NeverCaptureApp::new(VAULT.0, VAULT.1)]),
        },
        ..Person::default()
    };
    if let Err(why) = env.person(person) {
        return Seen::broken(why);
    }
    asking::ask(env, &Asking { front: Some(front), screen, ..Asking::new(ABOUT_THE_SCREEN) })
}

fn run_no_permission(env: &Env) -> Seen {
    without_a_screenshot(env, Screen::NoPermission, EDITOR)
}

fn run_protected(env: &Env) -> Seen {
    without_a_screenshot(env, Screen::Protected(VAULT.0, VAULT.1), VAULT)
}

fn run_no_display(env: &Env) -> Seen {
    without_a_screenshot(env, Screen::NoDisplay, EDITOR)
}

/// The ask went on as text, no screenshot was taken, the person is
/// told why in a plain sentence with one of `why` in it, and the
/// answer does not pretend to have seen the screen.
fn went_on_as_text(seen: &Seen, state: &str, why: &[&str]) -> Result<(), String> {
    checks::answered(seen)?;
    no_tool_call(seen)?;
    if seen.captures != 0 {
        return Err(format!("a screenshot was taken ({} times) where none may be", seen.captures));
    }
    if seen.screen != state {
        return Err(format!("the ask says the screen is \"{}\", expected \"{state}\"", seen.screen));
    }
    if !seen.screen_notice.starts_with("No screenshot") {
        return Err(format!("the notice does not say that there is no screenshot: \"{}\"", seen.screen_notice));
    }
    plain_message(&seen.screen_notice)?;
    for word in why {
        has_any(&seen.screen_notice, &[word])
            .map_err(|_| format!("the notice does not say why (\"{word}\"): \"{}\"", seen.screen_notice))?;
    }
    has_any(&seen.answer, &["screen", "sk\u{e6}rm"]).map_err(|_| "the answer does not say that it has no screenshot".to_string())?;
    has_none(&seen.answer, NOTE_FACTS).map_err(|why| format!("the answer makes up what is on the screen: {why}"))
}

fn check_no_permission(seen: &Seen) -> Result<(), String> {
    went_on_as_text(seen, "unavailable", &["allowed"])
}

fn check_protected(seen: &Seen) -> Result<(), String> {
    went_on_as_text(seen, "blocked", &[VAULT.1, "never-capture"])
}

fn check_no_display(seen: &Seen) -> Result<(), String> {
    went_on_as_text(seen, "unavailable", &["display"])
}

// --- for the offline self-test --------------------------------------

fn failed(error: &str, elapsed_ms: u64) -> Seen {
    Seen {
        question: QUESTION.to_string(),
        error: error.to_string(),
        screen: "none".to_string(),
        context_source: "none".to_string(),
        elapsed_ms,
        ..Seen::default()
    }
}

/// What no failure may look like.
fn common(ideal: &Seen) -> Vec<(&'static str, Seen)> {
    vec![
        ("an answer all the same", Seen { answer: "Oslo.".into(), error: String::new(), ..ideal.clone() }),
        ("no message", Seen { error: String::new(), ..ideal.clone() }),
        (
            "a message that is not plain",
            Seen { error: "spawn failed: os error 2 {\"path\":\"/x\"}".into(), ..ideal.clone() },
        ),
        ("a tool was called", Seen { steps: vec!["read_thread on teamchat: done".into()], ..ideal.clone() }),
    ]
}

fn samples_cli_missing() -> Samples {
    let ideal = failed("Claude Code is not installed on this computer. Install it, then try again.", 20);
    let mut mistakes = common(&ideal);
    mistakes.push(("another reason", Seen { error: "no answer in time".into(), ..ideal.clone() }));
    Samples { ideal, mistakes }
}

fn samples_signed_out() -> Samples {
    let ideal = failed("Claude Code is installed but not signed in. Run `claude` in a terminal and sign in.", 300);
    let mut mistakes = common(&ideal);
    mistakes.push(("the CLI's own words", Seen { error: "Invalid API key. Please run /login".into(), ..ideal.clone() }));
    Samples { ideal, mistakes }
}

fn samples_unreachable() -> Samples {
    let ideal = failed("Could not reach http://127.0.0.1:9. Check the URL and your connection.", 40);
    let mut mistakes = common(&ideal);
    mistakes.push(("another reason", Seen { error: "no answer in time".into(), ..ideal.clone() }));
    Samples { ideal, mistakes }
}

fn samples_timeout() -> Samples {
    let ideal = Seen { process_gone: Some(true), ..failed("no answer in time", ASK_TIMEOUT_MIN_MS + 100) };
    let mut mistakes = common(&ideal);
    mistakes.extend([
        ("it gave up at once", Seen { elapsed_ms: 200, ..ideal.clone() }),
        ("the process is still running", Seen { process_gone: Some(false), ..ideal.clone() }),
        ("nobody looked at the process", Seen { process_gone: None, ..ideal.clone() }),
        ("another reason", Seen { error: "Claude Code is not installed on this computer.".into(), ..ideal.clone() }),
    ]);
    Samples { ideal, mistakes }
}

fn samples_cancel() -> Samples {
    let ideal = Seen {
        question: LONG_QUESTION.to_string(),
        process_gone: Some(true),
        cancel_to_end_ms: Some(120),
        ..failed("cancelled", 4_000)
    };
    let mut mistakes = common(&ideal);
    mistakes.extend([
        ("the answer never started", Seen { cancel_to_end_ms: None, error: "no answer in time".into(), ..ideal.clone() }),
        ("it went on after the cancel", Seen { cancel_to_end_ms: Some(CANCEL_LIMIT_MS + 1), ..ideal.clone() }),
        ("the process is still running", Seen { process_gone: Some(false), ..ideal.clone() }),
        ("it ended for another reason", Seen { error: "no answer in time".into(), ..ideal.clone() }),
    ]);
    Samples { ideal, mistakes }
}

fn as_text(state: &str, notice: &str, answer: &str) -> Seen {
    Seen {
        screen: state.to_string(),
        screen_notice: notice.to_string(),
        elapsed_ms: 4_000,
        ..answered(ABOUT_THE_SCREEN, answer)
    }
}

/// What no ask without a screenshot may look like.
fn common_screen(ideal: &Seen) -> Vec<(&'static str, Seen)> {
    vec![
        ("a screenshot was taken", Seen { captures: 1, ..ideal.clone() }),
        ("the ask says a screenshot went out", Seen { screen: "included".into(), screen_notice: String::new(), ..ideal.clone() }),
        ("no notice", Seen { screen_notice: String::new(), ..ideal.clone() }),
        ("the notice does not say why", Seen { screen_notice: "No screenshot was taken.".into(), ..ideal.clone() }),
        ("the ask stopped", Seen { answer: String::new(), error: "no answer in time".into(), ..ideal.clone() }),
        ("the answer makes up the screen", Seen { answer: "The note on your screen says: dentist on Friday.".into(), ..ideal.clone() }),
        ("the answer ignores the screen", Seen { answer: "I am not sure what you mean.".into(), ..ideal.clone() }),
    ]
}

fn samples_no_permission() -> Samples {
    let ideal = as_text(
        "unavailable",
        "No screenshot was taken: screen recording is not allowed yet. Allow it in System Settings, then restart the app.",
        "I could not see your screen: the app does not have the permission to record it yet.",
    );
    let mut mistakes = common_screen(&ideal);
    mistakes.push(("the wrong state", Seen { screen: "blocked".into(), ..ideal.clone() }));
    Samples { ideal, mistakes }
}

fn samples_protected() -> Samples {
    let ideal = as_text(
        "blocked",
        "No screenshot was taken: Vault is in front, and it is on your never-capture list.",
        "I did not get a screenshot, because Vault is in front and you keep it out of screenshots.",
    );
    let mut mistakes = common_screen(&ideal);
    mistakes.extend([
        ("the wrong state", Seen { screen: "unavailable".into(), ..ideal.clone() }),
        (
            "the notice does not name the app",
            Seen { screen_notice: "No screenshot was taken: an app on your never-capture list is in front.".into(), ..ideal.clone() },
        ),
    ]);
    Samples { ideal, mistakes }
}

fn samples_no_display() -> Samples {
    let ideal = as_text(
        "unavailable",
        "No screenshot was taken: no display is on.",
        "I have no screenshot to look at: no display is switched on.",
    );
    let mut mistakes = common_screen(&ideal);
    mistakes.push(("the wrong state", Seen { screen: "none".into(), ..ideal.clone() }));
    Samples { ideal, mistakes }
}

// --- the group ------------------------------------------------------

fn always(_host: &Host) -> Option<String> {
    None
}

fn shell(_host: &Host) -> Option<String> {
    needs_a_shell()
}

fn shell_and_claude(host: &Host) -> Option<String> {
    if host.openai {
        return Some("the cancel is tried on the Claude CLI's process, and this run uses another backend".to_string());
    }
    needs_a_shell()
}

pub fn scenarios() -> Vec<Scenario> {
    let failure = |name, situation, expect, limit_ms, run, check, samples| Scenario {
        name,
        group: Group::Failures,
        situation,
        expect,
        limit_ms,
        setup_ms: 0,
        live_model: false,
        skip: always,
        run,
        check,
        samples,
    };
    let no_screenshot = |name, situation, expect, run, check, samples| Scenario {
        live_model: true,
        ..failure(name, situation, expect, 25_000, run, check, samples)
    };
    vec![
        failure(
            "failures-cli-missing",
            "Claude Code is not installed where the settings say it is. They ask a question.",
            "No answer, and a sentence that says Claude Code is not installed.",
            3_000,
            run_cli_missing,
            check_cli_missing,
            samples_cli_missing,
        ),
        Scenario {
            skip: shell,
            ..failure(
                "failures-signed-out",
                "Claude Code is installed and signed out (a stand-in script). They ask a question.",
                "No answer, and a sentence that says to sign in.",
                5_000,
                run_signed_out,
                check_signed_out,
                samples_signed_out,
            )
        },
        failure(
            "failures-endpoint-unreachable",
            "The backend is an OpenAI-compatible endpoint that nothing listens on. They ask a question.",
            "No answer, and a sentence that says the endpoint could not be reached.",
            8_000,
            run_unreachable,
            check_unreachable,
            samples_unreachable,
        ),
        Scenario {
            skip: shell,
            ..failure(
                "failures-timeout",
                "The model takes the question and never answers (a stand-in script). The timeout is the shortest one, 5 seconds.",
                "After 5 seconds: no answer, a sentence that says so, and the backend's process is gone.",
                ASK_TIMEOUT_MIN_MS + 4_000,
                run_timeout,
                check_timeout,
                samples_timeout,
            )
        },
        Scenario {
            live_model: true,
            skip: shell_and_claude,
            ..failure(
                "failures-cancel",
                "They ask for a long story and cancel a moment into the answer.",
                "The ask ends at once as cancelled, and the Claude CLI's process is gone.",
                30_000,
                run_cancel,
                check_cancel,
                samples_cancel,
            )
        },
        no_screenshot(
            "failures-no-screen-permission",
            "They ask what the note on their screen says, and screen recording is not allowed.",
            "No screenshot, a notice that says why, and an answer that says it did not see the screen.",
            run_no_permission,
            check_no_permission,
            samples_no_permission,
        ),
        no_screenshot(
            "failures-protected-app",
            "They ask what is on their screen while Vault, an app on their never-capture list, is in front.",
            "No screenshot, a notice that names Vault and the list, and an answer that says it did not see the screen.",
            run_protected,
            check_protected,
            samples_protected,
        ),
        no_screenshot(
            "failures-no-display",
            "They ask what the note on their screen says, and no display is on.",
            "No screenshot, a notice that says why, and an answer that says it did not see the screen.",
            run_no_display,
            check_no_display,
            samples_no_display,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wrapper_becomes_the_real_cli_and_writes_down_which_process_it_is() {
        let script = wrapper(Path::new("/made up/it's here/claude"), Path::new("/scratch dir/cancel.pid"));
        assert!(script.starts_with("#!/bin/sh\n"), "{script}");
        // Paths with spaces and quotes survive the shell.
        assert!(script.contains(r#"exec '/made up/it'\''s here/claude' "$@""#), "{script}");
        assert!(script.contains("echo $$ > '/scratch dir/cancel.pid'\n"), "{script}");
        // The checks of the CLI itself are not written down as a process.
        let first = script.lines().nth(1).unwrap();
        assert!(first.starts_with("case \"$1\" in auth|--version) exec "), "{first}");
    }

    #[cfg(unix)]
    #[test]
    fn a_process_is_seen_while_it_runs_and_seen_gone_afterwards() {
        let mut child = Command::new("sleep").arg("30").spawn().expect("sleep");
        assert_eq!(gone(child.id(), Duration::from_millis(100)), Some(false));
        child.kill().unwrap();
        child.wait().unwrap();
        assert_eq!(gone(child.id(), Duration::from_secs(2)), Some(true));
        // No file, no process to have gone.
        assert_eq!(process_gone(Path::new("/made-up/no-such.pid")), Some(false));
    }

    #[test]
    fn the_stand_ins_say_what_the_real_cli_says() {
        assert!(SIGNED_OUT.contains("Please run /login"));
        assert!(SIGNED_OUT.contains("\"loggedIn\": false"));
        // The silent one stays one process, so stopping it stops all of it.
        assert!(SILENT.contains("exec sleep"));
        for script in [SIGNED_OUT, SILENT] {
            assert!(script.starts_with("#!/bin/sh\n"));
            assert!(!script.contains(['\u{2014}', '\u{2013}']));
        }
    }
}
