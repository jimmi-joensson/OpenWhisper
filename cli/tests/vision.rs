//! End-to-end checks for `openwhisper vision` and for the screen
//! options of `openwhisper ask run` (TASK-98). Spawns the bin, like
//! the ask tests, and reads its output.
//!
//! Every invocation gets `OPENWHISPER_CONFIG_DIR` pointed at a folder
//! of its own under cargo's test tmp dir, so nothing here can read or
//! write a real `settings.json`.
//!
//! **No test in this file captures the screen.** `ask run --screen`
//! and any `--vision` run whose question would trigger a capture are
//! not run here: what they would capture is the screen of whoever
//! runs the tests. `vision probe` is run: it is a dry run that takes
//! no screenshot, and the tests assert on the shape of its report,
//! never on what is in front on this machine. The capture layers are
//! tested in core against a fake screen, and against the real one, on
//! pixels of the test's own windows only, in `vision_live.rs` (cargo
//! feature `vision-live-test`). No command here calls a model either:
//! where an answer is needed, `--claude-path` points at a shell
//! script that prints what the real CLI would stream.

use std::path::PathBuf;
use std::process::{Command, Output};

use serde_json::Value;

/// A scratch config folder for one test, removed again when the test
/// is done.
struct Config {
    dir: PathBuf,
}

impl Config {
    fn new(test: &str) -> Self {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("vision-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch config dir");
        Self { dir }
    }

    fn spawn(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_openwhisper"))
            .args(args)
            .env("OPENWHISPER_CONFIG_DIR", &self.dir)
            .env("OPENWHISPER_REFINE_API_KEY", "not-a-real-key")
            .env_remove("OPENWHISPER_VERBOSE")
            .output()
            .expect("spawn openwhisper")
    }

    /// Run with `--json` and parse stdout; the command must succeed.
    fn json(&self, args: &[&str]) -> Value {
        let mut full = vec!["--json"];
        full.extend_from_slice(args);
        let out = self.spawn(&full);
        assert!(
            out.status.success(),
            "`openwhisper {}` failed\nstderr:\n{}",
            full.join(" "),
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
            panic!(
                "`openwhisper {}`: stdout is not JSON ({e}):\n{}",
                full.join(" "),
                String::from_utf8_lossy(&out.stdout)
            )
        })
    }

    /// Run a command that must fail; returns its stderr.
    fn fails(&self, args: &[&str]) -> String {
        let out = self.spawn(args);
        assert!(!out.status.success(), "`openwhisper {}` should have failed", args.join(" "));
        String::from_utf8_lossy(&out.stderr).to_string()
    }

    fn settings_file(&self) -> Value {
        let file = std::fs::read_to_string(self.dir.join("settings.json")).expect("settings.json");
        serde_json::from_str(&file).expect("settings.json is JSON")
    }
}

impl Drop for Config {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The trigger of a config nobody touched.
const DEFAULT_TRIGGER: &str = if cfg!(target_os = "windows") { "off" } else { "phrases" };

fn strings(value: &Value) -> Vec<String> {
    value.as_array().expect("an array").iter().map(|v| v.as_str().expect("a string").to_string()).collect()
}

fn app_ids(value: &Value) -> Vec<String> {
    value.as_array().expect("an array").iter().map(|v| v["id"].as_str().expect("an id").to_string()).collect()
}

#[test]
fn a_fresh_config_has_the_platforms_trigger_and_the_built_in_lists() {
    let cfg = Config::new("fresh");
    let status = cfg.json(&["vision", "status"]);
    // Phrases on macOS; off on Windows until capture has been
    // verified on a real machine there.
    assert_eq!(status["trigger"], DEFAULT_TRIGGER);
    assert_eq!(status["phrases_are_default"], true);
    assert_eq!(status["never_capture_is_default"], true);
    let phrases = strings(&status["phrases"]);
    assert_eq!(status["phrase_count"], phrases.len());
    // English and Danish.
    for must in ["on my screen", "this error", "på min skærm", "denne fejl"] {
        assert!(phrases.iter().any(|p| p == must), "missing \"{must}\"");
    }
    // The common password managers, by this platform's ids.
    let apps = app_ids(&status["never_capture"]);
    assert!(apps.len() >= 5, "{apps:?}");
    assert!(apps.iter().any(|id| id.contains("1password")), "{apps:?}");
    assert!(apps.iter().any(|id| id.contains("bitwarden")), "{apps:?}");
    // The permission is one of the three states, and the status says
    // whose permission a command line program uses.
    let permission = status["permission"].as_str().expect("permission");
    assert!(["granted", "denied", "unknown"].contains(&permission), "{permission}");
    assert!(status["permission_note"].as_str().is_some_and(|n| n.contains("terminal")), "{status}");
    // Reading the status wrote nothing.
    assert!(!cfg.dir.join("settings.json").exists());

    // For people: the same, as lines.
    let out = cfg.spawn(&["vision", "status"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains(&format!("trigger        {DEFAULT_TRIGGER} (")), "{stdout}");
    assert!(stdout.contains(&format!("phrases        {} (the built-in list)", phrases.len())), "{stdout}");
    assert!(stdout.contains("(the built-in list)\n"), "{stdout}");
    assert!(stdout.contains("for this process"), "{stdout}");
    assert!(stdout.contains("Screen Recording permission of the terminal it runs in"), "{stdout}");

    // The helper window of the pixel test is not something to offer.
    let help = cfg.spawn(&["vision", "--help"]);
    let help = String::from_utf8_lossy(&help.stdout);
    assert!(help.contains("probe") && help.contains("never-capture"), "{help}");
    assert!(!help.contains("test-window"), "{help}");
}

#[test]
fn config_sets_the_trigger_and_nothing_else() {
    let cfg = Config::new("config");
    // Something another command wrote first.
    cfg.json(&["ask", "config", "--instructions", "Answer in Danish."]);

    let shown = cfg.json(&["vision", "config"]);
    assert_eq!(shown["trigger"], DEFAULT_TRIGGER);
    assert_eq!((&shown["phrases"], &shown["never_capture"]), (&Value::Null, &Value::Null));
    assert!(cfg.settings_file().get("vision").is_none_or(Value::is_null), "printing the block wrote it");

    for trigger in ["off", "smart", "always", "phrases"] {
        let set = cfg.json(&["vision", "config", "--trigger", trigger]);
        assert_eq!(set["trigger"], trigger);
        let file = cfg.settings_file();
        assert_eq!(file["vision"], set);
        // The lists were not touched: still the built-in ones.
        assert_eq!(file["vision"]["phrases"], Value::Null);
        assert_eq!(file["vision"]["never_capture"], Value::Null);
        assert_eq!(file["ask"]["instructions"], "Answer in Danish.");
        assert_eq!(cfg.json(&["vision", "status"])["trigger"], trigger);
    }

    // A trigger that does not exist is refused, and nothing is written.
    cfg.json(&["vision", "config", "--trigger", "smart"]);
    assert!(cfg.fails(&["vision", "config", "--trigger", "sometimes"]).contains("sometimes"));
    assert_eq!(cfg.json(&["vision", "config"])["trigger"], "smart");

    // A later save of another block keeps the vision block.
    cfg.json(&["ask", "config", "--enabled", "true"]);
    assert_eq!(cfg.settings_file()["vision"]["trigger"], "smart");
}

#[test]
fn phrases_can_be_added_removed_and_reset() {
    let cfg = Config::new("phrases");
    let built_in = strings(&cfg.json(&["vision", "phrases", "list"])["phrases"]);
    assert!(built_in.len() >= 10);
    assert!(!cfg.dir.join("settings.json").exists(), "listing wrote the settings");

    // Adding one that is already there writes nothing: the list stays
    // the built-in one, so later defaults still reach this person.
    let same = cfg.json(&["vision", "phrases", "add", "ON MY  SCREEN"]);
    assert_eq!(same["default"], true);
    assert!(!cfg.dir.join("settings.json").exists());

    // A new one: tidied, at the end, and the list is theirs from now.
    let added = cfg.json(&["vision", "phrases", "add", "  look at   this window "]);
    assert_eq!(added["default"], false);
    let list = strings(&added["phrases"]);
    assert_eq!(list.len(), built_in.len() + 1);
    assert_eq!(list.last().map(String::as_str), Some("look at this window"));
    assert_eq!(strings(&cfg.settings_file()["vision"]["phrases"]), list);
    // It decides from now on.
    let check = cfg.json(&["vision", "check", "can you look at this window for me"]);
    assert_eq!((check["decision"].as_str(), check["phrase"].as_str()), (Some("capture"), Some("look at this window")));

    // Removing ignores case; what is not there cannot be removed.
    let removed = cfg.json(&["vision", "phrases", "remove", "On My Screen"]);
    let list = strings(&removed["phrases"]);
    assert_eq!(list.len(), built_in.len());
    assert!(!list.iter().any(|p| p == "on my screen"));
    assert!(cfg.fails(&["vision", "phrases", "remove", "on my screen"]).contains("is not one of the phrases"));
    assert!(cfg.fails(&["vision", "phrases", "add", "   "]).contains("at least one word"));
    // "what is on my screen" is still caught, by the longer phrase.
    assert_eq!(cfg.json(&["vision", "check", "what's on my screen"])["phrase"], "what's on my screen");
    assert_eq!(cfg.json(&["vision", "check", "it is on my screen"])["decision"], "no-capture");

    // Back to the built-in list.
    let reset = cfg.json(&["vision", "phrases", "reset"]);
    assert_eq!(reset["default"], true);
    assert_eq!(strings(&reset["phrases"]), built_in);
    assert_eq!(cfg.settings_file()["vision"]["phrases"], Value::Null);
    // The trigger was never touched.
    assert_eq!(cfg.settings_file()["vision"]["trigger"], "phrases");

    // For people: one phrase per line.
    let out = cfg.spawn(&["vision", "phrases", "list"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout.lines().count(), built_in.len(), "{stdout}");
    assert!(stdout.lines().any(|l| l == "på skærmen"), "{stdout}");
}

#[test]
fn never_capture_apps_can_be_added_removed_and_reset() {
    let cfg = Config::new("never-capture");
    let built_in = app_ids(&cfg.json(&["vision", "never-capture", "list"])["never_capture"]);
    assert!(!cfg.dir.join("settings.json").exists(), "listing wrote the settings");

    // Ids are kept in lower case; a name is optional.
    let added = cfg.json(&["vision", "never-capture", "add", " Com.Example.Vault ", "--name", "Example Vault"]);
    assert_eq!(added["default"], false);
    let last = added["never_capture"].as_array().unwrap().last().unwrap().clone();
    assert_eq!((last["id"].as_str(), last["name"].as_str()), (Some("com.example.vault"), Some("Example Vault")));
    let added = cfg.json(&["vision", "never-capture", "add", "ledger.exe"]);
    let ids = app_ids(&added["never_capture"]);
    assert_eq!(ids.len(), built_in.len() + 2);
    assert_eq!(app_ids(&cfg.settings_file()["vision"]["never_capture"]), ids);
    assert_eq!(cfg.json(&["vision", "status"])["never_capture_is_default"], false);

    // Adding it again changes nothing; a new name replaces the old.
    let again = cfg.json(&["vision", "never-capture", "add", "com.example.vault"]);
    assert_eq!(app_ids(&again["never_capture"]), ids);
    let renamed = cfg.json(&["vision", "never-capture", "add", "com.example.vault", "--name", "Vault"]);
    let entry = renamed["never_capture"].as_array().unwrap().iter().find(|a| a["id"] == "com.example.vault").unwrap();
    assert_eq!(entry["name"], "Vault");

    // Removing: by id, any case. A built-in entry can go too.
    let removed = cfg.json(&["vision", "never-capture", "remove", "LEDGER.EXE"]);
    assert!(!app_ids(&removed["never_capture"]).iter().any(|id| id == "ledger.exe"));
    let removed = cfg.json(&["vision", "never-capture", "remove", &built_in[0]]);
    assert!(!app_ids(&removed["never_capture"]).contains(&built_in[0]));
    assert!(cfg.fails(&["vision", "never-capture", "remove", "ledger.exe"]).contains("is not on the list"));
    assert!(cfg.fails(&["vision", "never-capture", "add", "  "]).contains("needs an id"));

    // Back to the built-in list.
    let reset = cfg.json(&["vision", "never-capture", "reset"]);
    assert_eq!(reset["default"], true);
    assert_eq!(app_ids(&reset["never_capture"]), built_in);
    assert_eq!(cfg.settings_file()["vision"]["never_capture"], Value::Null);
}

#[test]
fn check_decides_in_english_and_danish_without_capturing() {
    let cfg = Config::new("check");
    let decide = |args: &[&str]| {
        let mut full = vec!["vision", "check"];
        full.extend_from_slice(args);
        let value = cfg.json(&full);
        (
            value["decision"].as_str().expect("decision").to_string(),
            value["phrase"].as_str().map(str::to_string),
            value["trigger"].as_str().expect("trigger").to_string(),
        )
    };
    let capture = |phrase: &str| ("capture".to_string(), Some(phrase.to_string()), "phrases".to_string());
    let nothing = ("no-capture".to_string(), None, "phrases".to_string());

    // The default trigger: a phrase in the question, in either language.
    assert_eq!(decide(&["What is on my screen?"]), capture("on my screen"));
    assert_eq!(decide(&["can you look at the screen and tell me what is wrong"]), capture("look at the screen"));
    assert_eq!(decide(&["what does this error mean"]), capture("this error"));
    assert_eq!(decide(&["hvad står der på min skærm"]), capture("på min skærm"));
    assert_eq!(decide(&["Kan du svare på denne besked?"]), capture("denne besked"));
    assert_eq!(decide(&["tag et skærmbillede og forklar det"]), capture("skærmbillede"));
    // No phrase: no screenshot.
    assert_eq!(decide(&["what is the capital of france"]), nothing);
    assert_eq!(decide(&["hvad er hovedstaden i frankrig"]), nothing);
    assert_eq!(decide(&["how many screens can I connect"]), nothing);
    assert_eq!(decide(&["hvor mange skærme kan jeg tilslutte"]), nothing);

    // The other triggers, for this one check.
    let with = |text: &str, trigger: &str| {
        let (decision, phrase, used) = decide(&[text, "--trigger", trigger]);
        assert_eq!(used, trigger);
        (decision, phrase)
    };
    assert_eq!(with("look at my screen", "off"), ("no-capture".to_string(), Some("look at my screen".to_string())));
    assert_eq!(with("write a haiku about rain", "smart"), ("ask-model".to_string(), None));
    assert_eq!(with("hvad betyder denne fejl", "smart"), ("capture".to_string(), Some("denne fejl".to_string())));
    assert_eq!(with("write a haiku about rain", "always"), ("capture".to_string(), None));
    assert!(cfg.fails(&["vision", "check", "hello", "--trigger", "sometimes"]).contains("sometimes"));
    // Checking saved nothing, the override included.
    assert!(!cfg.dir.join("settings.json").exists());

    // The saved trigger is the one used without an override.
    cfg.json(&["vision", "config", "--trigger", "off"]);
    assert_eq!(decide(&["what is on my screen"]), ("no-capture".to_string(), Some("on my screen".to_string()), "off".to_string()));

    // For people: the decision, the phrase, and that nothing happened.
    let out = cfg.spawn(&["vision", "check", "se på min skærm", "--trigger", "phrases"]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("decision  capture: a screenshot would go with this question"), "{stdout}");
    assert!(stdout.contains("phrase    se på min skærm"), "{stdout}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("nothing was captured and nothing was sent"));
}

/// `vision probe` is a dry run. It reports what a capture would look
/// at and takes none, so it can run here. The assertions are on the
/// shape of the report: what is in front on the machine that runs the
/// tests is nobody's business, and is not printed on a failure.
#[test]
fn probe_is_a_dry_run_that_reports_and_captures_nothing() {
    let cfg = Config::new("probe");
    cfg.json(&["vision", "never-capture", "add", "com.example.vault", "--name", "Vault"]);
    let seen = cfg.json(&["vision", "probe"]);
    assert_eq!(seen["captured"], false);
    assert_eq!(seen["dry_run"], true);
    let permission = seen["permission"].as_str().expect("permission");
    assert!(["granted", "denied", "unknown"].contains(&permission), "{permission}");
    // The display: measured, or a reason why not.
    match &seen["display"] {
        Value::Null => assert!(seen["display_problem"].as_str().is_some_and(|p| !p.is_empty())),
        display => {
            assert!(display["width"].as_u64().is_some_and(|w| w >= 1));
            assert!(display["height"].as_u64().is_some_and(|h| h >= 1));
            assert!(display["point_width"].as_f64().is_some_and(|w| w > 0.0));
        }
    }
    // The apps in front, each with whether it is on the list; or a
    // reason.
    let front = seen["front"].as_array().expect("front");
    assert!(!front.is_empty() || seen["front_problem"].as_str().is_some_and(|p| !p.is_empty()));
    assert!(front.iter().all(|app| app["listed"].is_boolean() && app["id"].is_string()));
    // Every app on the list in force, the added one last, with
    // whether it is open and how many windows it has on screen.
    let listed = seen["listed"].as_array().expect("listed");
    assert_eq!(listed.len(), app_ids(&cfg.json(&["vision", "never-capture", "list"])["never_capture"]).len());
    let vault = listed.last().unwrap();
    assert_eq!((vault["id"].as_str(), vault["name"].as_str()), (Some("com.example.vault"), Some("Vault")));
    // An app that does not exist is not open and has no window.
    assert_eq!(vault["running"], false);
    assert!(vault["windows"].is_null() || vault["windows"] == 0, "{}", vault["windows"]);
    assert_eq!(seen["never_capture_is_default"], false);
    // Whether a capture would go ahead, and why not when not.
    assert_eq!(seen["would_capture"].as_bool(), Some(seen["refusal"].is_null()));
    assert_eq!(seen["refusal_message"].is_null(), seen["refusal"].is_null());
    // No question was given.
    assert_eq!(seen["decision"], Value::Null);
    assert_eq!(seen["trigger"], DEFAULT_TRIGGER);
    // Nothing of an image: no key of the report sounds like one.
    for key in seen.as_object().unwrap().keys() {
        for word in ["png", "image", "pixel", "bytes", "capture_ms"] {
            assert!(!key.contains(word), "the report has a field \"{key}\"");
        }
    }

    // With a question: what the trigger would decide for it.
    let seen = cfg.json(&["vision", "probe", "--text", "what does it say on my screen", "--trigger", "phrases"]);
    assert_eq!(seen["captured"], false);
    assert_eq!(seen["trigger"], "phrases");
    assert_eq!(seen["decision"]["decision"], "capture");
    assert_eq!(seen["decision"]["phrase"], "on my screen");
    let seen = cfg.json(&["vision", "probe", "--text", "two plus two", "--trigger", "smart"]);
    assert_eq!(seen["decision"]["decision"], "ask-model");
    assert_eq!(seen["decision"]["phrase"], Value::Null);
    assert!(cfg.fails(&["vision", "probe", "--trigger", "sometimes"]).contains("sometimes"));

    // For people: it says first that nothing was captured.
    let out = cfg.spawn(&["vision", "probe", "--text", "two plus two", "--trigger", "off"]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines[0], "captured       nothing: this is a dry run, it takes no screenshot");
    for start in ["permission     ", "display        ", "never capture  ", "would capture  ", "trigger        off ("] {
        assert!(lines.iter().any(|l| l.starts_with(start)), "no line starts with {start:?}");
    }
    assert!(lines.iter().any(|l| l.starts_with("in front       ")), "no line about the app in front");
    assert!(lines.iter().any(|l| l.trim() == "no-capture: no screenshot for this question"));
    assert!(String::from_utf8_lossy(&out.stderr).contains("nothing was captured and nothing was sent"));
    // The old way in, a capture in a copy of the program, is gone.
    assert!(cfg.fails(&["vision", "probe", "--inner"]).contains("--inner"));
    // And a dry run wrote nothing but what `never-capture add` did.
    assert_eq!(cfg.settings_file()["vision"]["never_capture"].as_array().unwrap().len(), listed.len());
}

#[test]
fn the_helper_window_and_the_eval_refuse_what_they_cannot_do() {
    let cfg = Config::new("refusals");
    // The helper window of the pixel test: this program does not
    // have it unless it was built with the feature for that test.
    let err = cfg.fails(&[
        "vision", "test-window", "--color", "pink", "--x", "0", "--y", "0", "--width", "10", "--height", "10",
    ]);
    if cfg!(feature = "vision-live-test") {
        // A colour that is none: refused before any window exists.
        assert!(err.contains("six hex digits"), "{err}");
    } else {
        assert!(err.contains("unrecognized subcommand"), "{err}");
    }
    // The eval: a prefix that matches nothing lists the sets, a
    // trigger without screenshots is refused, and no case runs
    // without a backend that is ready.
    let err = cfg.fails(&["vision", "eval", "--only", "nope", "--claude-path", "/nonexistent/claude"]);
    assert!(err.contains("no eval case starts with \"nope\""), "{err}");
    assert!(err.contains("smart, phrases, read and inject"), "{err}");
    let err = cfg.fails(&["vision", "eval", "--trigger", "off", "--claude-path", "/nonexistent/claude"]);
    assert!(err.contains("no ask includes a screenshot"), "{err}");
    assert!(cfg.fails(&["vision", "eval", "--trigger", "sometimes"]).contains("sometimes"));
    assert!(cfg.fails(&["vision", "eval", "--claude-path", "/nonexistent/claude"]).contains("is not ready"));
}

/// The commands that need an answer, against a script standing in for
/// the Claude CLI.
#[cfg(unix)]
mod with_a_fake_cli {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// Answers `claude auth status` and `--version`, records its
    /// arguments, the system prompt and each line it is sent, and
    /// streams the same answer to every question the way the real CLI
    /// does.
    const FAKE: &str = r#"#!/bin/sh
here="$(dirname "$0")"
if [ "$1" = "auth" ]; then echo '{"loggedIn": true, "authMethod": "claude.ai", "subscriptionType": "max"}'; exit 0; fi
if [ "$1" = "--version" ]; then echo '9.9.9 (Fake Claude)'; exit 0; fi
echo "$@" > "$here/args.txt"
prev=""; for a in "$@"; do if [ "$prev" = "--system-prompt-file" ]; then cp "$a" "$here/system.txt"; fi; prev="$a"; done
while read -r line; do
  printf '%s\n' "$line" > "$here/stdin.txt"
  echo '{"type":"stream_event","event":{"type":"message_start"}}'
  echo '{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"South has the most orders."}}}'
  echo '{"type":"result","subtype":"success","is_error":false,"result":"South has the most orders.","total_cost_usd":0.0025,"usage":{"input_tokens":2,"cache_creation_input_tokens":560,"cache_read_input_tokens":40,"output_tokens":11}}'
done
"#;

    fn fake_cli(cfg: &Config) -> String {
        let dir = cfg.dir.join("bin");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("claude");
        std::fs::write(&path, FAKE).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path.display().to_string()
    }

    fn recorded(cfg: &Config, name: &str) -> String {
        std::fs::read_to_string(cfg.dir.join("bin").join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
    }

    /// The content blocks of the last line the fake was sent.
    fn sent_blocks(cfg: &Config) -> Vec<Value> {
        let sent: Value = serde_json::from_str(&recorded(cfg, "stdin.txt")).expect("the user line");
        sent["message"]["content"].as_array().expect("content blocks").clone()
    }

    #[test]
    fn a_run_without_a_screen_option_never_involves_the_screen() {
        let cfg = Config::new("run-plain");
        let claude = fake_cli(&cfg);
        // Even with "always" saved: a run looks at the screen only
        // when it is told to on the command line.
        cfg.json(&["vision", "config", "--trigger", "always"]);
        let record = cfg.json(&["ask", "run", "look at my screen", "--claude-path", &claude]);
        assert_eq!(record["screen"], "none");
        assert_eq!(record["screen_notice"], "");
        assert_eq!(record["vision"], "none");
        let blocks = sent_blocks(&cfg);
        assert_eq!(blocks.len(), 1, "{blocks:?}");
        assert_eq!(blocks[0]["type"], "text");
        // The prompt is the one from before Vision.
        let system = recorded(&cfg, "system.txt");
        assert!(system.contains("You cannot see the person's screen"), "{system}");
        assert!(!system.contains("screenshot"), "{system}");

        let out = cfg.spawn(&["ask", "run", "look at my screen", "--claude-path", &claude]);
        assert!(out.status.success());
        assert!(!String::from_utf8_lossy(&out.stderr).contains("screen:"));
    }

    #[test]
    fn run_with_vision_off_never_captures_even_when_asked_to_look() {
        let cfg = Config::new("run-off");
        let claude = fake_cli(&cfg);
        let record = cfg.json(&["ask", "run", "look at my screen", "--vision", "off", "--claude-path", &claude]);
        assert_eq!(record["answer"], "South has the most orders.");
        assert_eq!(record["screen"], "none");
        assert_eq!(record["screen_notice"], "");
        assert_eq!(record["vision"], "off");
        // The question went out as text and nothing else.
        let blocks = sent_blocks(&cfg);
        assert_eq!(blocks.len(), 1, "{blocks:?}");
        assert_eq!(blocks[0]["text"], "<question>\nlook at my screen\n</question>");
        let system = recorded(&cfg, "system.txt");
        assert!(system.contains("You cannot see the person's screen"), "{system}");
        assert!(!cfg.dir.join("settings.json").exists(), "the override was saved");

        // For people: it says so before the question leaves.
        let out = cfg.spawn(&["ask", "run", "look at my screen", "--vision", "off", "--claude-path", &claude]);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "{stderr}");
        assert!(stderr.contains("screen: not included (trigger: off)"), "{stderr}");
        assert!(!stderr.contains("screen: capturing") && !stderr.contains("screen: included"), "{stderr}");
        let said = stderr.find("screen: not included").unwrap();
        assert!(said < stderr.find("first text after").unwrap(), "{stderr}");

        // "saved" is the trigger in the settings.
        cfg.json(&["vision", "config", "--trigger", "off"]);
        let record = cfg.json(&["ask", "run", "what is on my screen", "--vision", "saved", "--claude-path", &claude]);
        assert_eq!((record["vision"].as_str(), record["screen"].as_str()), (Some("off"), Some("none")));
    }

    #[test]
    fn run_with_the_phrases_trigger_and_no_phrase_sends_text_only() {
        let cfg = Config::new("run-phrases");
        let claude = fake_cli(&cfg);
        // No phrase in the question, so nothing is captured. (A
        // question with a phrase is not run here: it would capture
        // the screen of whoever runs the tests.)
        let out = cfg.spawn(&["ask", "run", "which region has the most orders", "--vision", "phrases", "--claude-path", &claude]);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "{stderr}");
        assert!(stderr.contains("screen: not included (trigger: phrases, and the question has none of them)"), "{stderr}");
        assert!(!stderr.contains("screen: capturing"), "{stderr}");
        let blocks = sent_blocks(&cfg);
        assert_eq!(blocks.len(), 1, "{blocks:?}");
        let record =
            cfg.json(&["ask", "run", "which region has the most orders", "--vision", "phrases", "--claude-path", &claude]);
        assert_eq!((record["vision"].as_str(), record["screen"].as_str()), (Some("phrases"), Some("none")));

        // What cannot be done says why, before anything is sent.
        let err = cfg.fails(&["ask", "run", "hi", "--vision", "sometimes", "--claude-path", &claude]);
        assert!(err.contains("sometimes") && !err.contains("question goes to"), "{err}");
        let err = cfg.fails(&["ask", "run", "hi", "--screen", "--vision", "off", "--claude-path", &claude]);
        assert!(err.contains("cannot be used with"), "{err}");
    }

    /// The fake, with an account that runs into its rate limit: it
    /// answers two questions (each case is a process of its own, so
    /// the count is kept in a file) and refuses every one after that
    /// the way the real CLI reports a 429.
    fn fake_cli_with_a_limit(cfg: &Config) -> String {
        let limited = FAKE.replace(
            "while read -r line; do\n",
            "while read -r line; do\n  n=$(cat \"$here/asked.txt\" 2>/dev/null || echo 0); n=$((n+1)); echo \"$n\" > \"$here/asked.txt\"\n  \
if [ \"$n\" -gt 2 ]; then echo '{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":true,\"result\":\"API Error: 429 rate_limit_error: this request would exceed your rate limit\"}'; continue; fi\n",
        );
        assert!(limited.contains("asked.txt"), "the fake changed shape");
        let path = cfg.dir.join("bin").join("claude-limited");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, limited).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path.display().to_string()
    }

    #[test]
    fn eval_stops_at_a_rate_limit_instead_of_counting_failures() {
        let cfg = Config::new("eval-limit");
        let claude = fake_cli_with_a_limit(&cfg);
        // Ten cases; the third question is refused.
        let out = cfg.spawn(&["vision", "eval", "--only", "smart-plain", "--claude-path", &claude]);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert_eq!(out.status.code(), Some(2), "{stdout}");
        assert!(stdout.contains("PASS  smart-plain-en-capital"), "{stdout}");
        assert!(stdout.contains("PASS  smart-plain-en-haiku"), "{stdout}");
        // The refused case is not a failed case, and nothing after it
        // was sent.
        assert!(stdout.contains("STOP  smart-plain-en-http-404"), "{stdout}");
        assert!(!stdout.contains("FAIL"), "{stdout}");
        assert!(!stdout.contains("smart-plain-en-translate"), "{stdout}");
        assert!(stdout.contains("2/2 passed"), "{stdout}");
        assert!(stdout.contains("stopped: the backend reported a spend or rate limit at \"smart-plain-en-http-404\""), "{stdout}");
        assert!(stdout.contains("8 cases were not run, nothing more was sent."), "{stdout}");
        assert_eq!(recorded(&cfg, "asked.txt").trim(), "3");

        // The same as a record.
        std::fs::remove_file(cfg.dir.join("bin").join("asked.txt")).unwrap();
        let out = cfg.spawn(&["--json", "vision", "eval", "--only", "smart-plain", "--claude-path", &claude]);
        assert_eq!(out.status.code(), Some(2));
        let report: Value = serde_json::from_slice(&out.stdout).expect("a JSON report");
        assert_eq!((report["passed"].as_u64(), report["total"].as_u64()), (Some(2), Some(2)));
        assert_eq!(report["stopped"]["case"], "smart-plain-en-http-404");
        assert_eq!(report["stopped"]["not_run"], 8);
        assert!(report["stopped"]["reason"].as_str().is_some_and(|r| r.contains("429")), "{report}");
        assert_eq!(recorded(&cfg, "asked.txt").trim(), "3");
        // A run that was not stopped says so.
        let whole = cfg.json(&["vision", "eval", "--only", "read-table", "--claude-path", &fake_cli(&cfg)]);
        assert_eq!(whole["stopped"], Value::Null);
    }

    #[test]
    fn eval_sends_a_synthetic_image_and_reports_per_set() {
        let cfg = Config::new("eval");
        let claude = fake_cli(&cfg);
        // The fake always answers "South has the most orders.": right
        // for the table.
        let report = cfg.json(&["vision", "eval", "--only", "read-table", "--claude-path", &claude]);
        assert_eq!((report["passed"].as_u64(), report["total"].as_u64()), (Some(1), Some(1)));
        assert_eq!(report["sets"]["read"]["passed"], 1);
        assert_eq!(report["smart"], Value::Null);
        assert_eq!(report["image_trigger"], "phrases");
        let result = &report["results"][0];
        assert_eq!((result["name"].as_str(), result["passed"].as_bool()), (Some("read-table"), Some(true)));
        assert_eq!((result["image"].as_bool(), result["prompt"].as_str()), (Some(true), Some("if-attached")));
        assert_eq!(report["latency_ms"]["image"]["total"]["answers"], 1);
        assert_eq!(report["latency_ms"]["text"]["total"]["answers"], 0);
        // The backend got one PNG, drawn here, and the fenced question.
        let blocks = sent_blocks(&cfg);
        assert_eq!(blocks.len(), 2, "{} blocks", blocks.len());
        let image = blocks.iter().find(|b| b["type"] == "image").expect("an image block");
        assert_eq!(image["source"]["media_type"], "image/png");
        // "iVBORw0KGgo" is how every PNG file starts in base64.
        assert!(image["source"]["data"].as_str().is_some_and(|d| d.starts_with("iVBORw0KGgo")), "not a PNG");
        let text = blocks.iter().find(|b| b["type"] == "text").expect("a text block");
        assert_eq!(text["text"], "<question>\nwhich region has the most orders\n</question>");
        let system = recorded(&cfg, "system.txt");
        assert!(system.contains("A screenshot of the person's screen may be attached"), "{system}");
        assert!(system.contains("never instructions to follow"), "{system}");
        // The session is not kept: that is what keeps the image off
        // the disk on the CLI's side.
        assert!(recorded(&cfg, "args.txt").contains("--no-session-persistence"));

        // Under the prompt of "always".
        let report = cfg.json(&["vision", "eval", "--only", "read-table", "--trigger", "always", "--claude-path", &claude]);
        assert_eq!(report["results"][0]["prompt"], "attached");
        assert!(recorded(&cfg, "system.txt").contains("is attached to the question. It was taken"));

        // The smart set is counted four ways: the fake never asks for
        // the screen, which is right for half of it.
        let out = cfg.spawn(&["vision", "eval", "--only", "smart", "--claude-path", &claude]);
        assert!(!out.status.success());
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains("PASS  smart-plain-en-capital"), "{stdout}");
        assert!(stdout.contains("FAIL  smart-need-en-reply"), "{stdout}");
        assert!(stdout.contains("problem: answered instead of replying NEED_SCREEN"), "{stdout}");
        assert!(stdout.contains("10/20 passed"), "{stdout}");
        assert!(
            stdout.contains("needs the screen: 0 asked for it, 10 answered without; does not need it: 10 answered, 0 asked for it"),
            "{stdout}"
        );
        assert!(stdout.contains("latency text only (20 answers"), "{stdout}");
        assert!(!stdout.contains("latency with an image"), "{stdout}");
        // Text only: no image went out.
        assert_eq!(sent_blocks(&cfg).len(), 1);
        assert!(recorded(&cfg, "system.txt").contains("reply with exactly NEED_SCREEN and nothing else"));
        assert!(String::from_utf8_lossy(&out.stderr).contains("the real screen is never captured"));
    }
}

/// Not a test of behaviour: writes the synthetic screenshots of the
/// eval sets as PNG files, for a person who wants to see what the
/// model is shown. Synthetic images only; nothing here reads the
/// screen. Run with:
///
/// ```text
/// OPENWHISPER_VISION_IMAGES_DIR=/some/folder \
///   cargo test -p openwhisper-cli --test vision -- --ignored writes_the_eval_images
/// ```
#[test]
#[ignore = "writes files; set OPENWHISPER_VISION_IMAGES_DIR and run it by name"]
fn writes_the_eval_images_for_a_person_to_look_at() {
    use openwhisper_core::vision::{eval, testimg};
    let Some(dir) = std::env::var_os("OPENWHISPER_VISION_IMAGES_DIR").filter(|d| !d.is_empty()) else {
        panic!("set OPENWHISPER_VISION_IMAGES_DIR to the folder the images go to");
    };
    let dir = PathBuf::from(dir);
    std::fs::create_dir_all(&dir).expect("create the folder");
    let mut written = 0;
    for case in eval::cases() {
        let Some(scene) = case.scene() else { continue };
        let png = testimg::png(&scene).expect("draw the image");
        std::fs::write(dir.join(format!("{}.png", case.name)), png).expect("write the image");
        written += 1;
    }
    assert!(written >= 11, "{written} images");
}
