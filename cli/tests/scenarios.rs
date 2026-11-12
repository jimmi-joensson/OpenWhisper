//! End-to-end checks for `openwhisper scenarios` (TASK-103). Spawns
//! the bin, like the ask and agent tests, and reads its output.
//!
//! No command in this file calls a model, a real connection or a
//! real screen: the offline self-test asks nothing, the failure
//! scenarios that are run here ask no model by design, and where an
//! answer is needed `--claude-path` points at a shell script that
//! prints what the real CLI would stream.
//!
//! The runner makes its own scratch config folder. The tests that
//! name one (`--config-dir`) put it under cargo's test tmp dir. The
//! real config folders are only ever named, to prove that they are
//! refused: nothing in them is opened, and nothing is written.

use std::path::PathBuf;
use std::process::{Command, Output};

use serde_json::Value;

/// A folder of its own for one test, removed again when the test is
/// done.
struct Scratch {
    dir: PathBuf,
}

impl Scratch {
    fn new(test: &str) -> Self {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("scenarios-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        Self { dir }
    }

    /// A folder next to the config folder that must stay untouched:
    /// what `OPENWHISPER_CONFIG_DIR` points at. The runner has a
    /// folder of its own and does not use this one.
    fn decoy(&self) -> PathBuf {
        let decoy = self.dir.join("decoy-config");
        std::fs::create_dir_all(&decoy).unwrap();
        decoy
    }

    fn spawn(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_openwhisper"))
            .args(args)
            .env("OPENWHISPER_CONFIG_DIR", self.decoy())
            .env("OPENWHISPER_REFINE_API_KEY", "not-a-real-key")
            .output()
            .expect("spawn openwhisper")
    }

    /// Run a command that must succeed; returns its stdout.
    fn ok(&self, args: &[&str]) -> String {
        let out = self.spawn(args);
        assert!(
            out.status.success(),
            "`openwhisper {}` failed\nstdout:\n{}\nstderr:\n{}",
            args.join(" "),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).to_string()
    }

    /// Run with `--json` and parse stdout, whatever the exit code.
    fn json(&self, args: &[&str]) -> (Option<i32>, Value) {
        let mut full = vec!["--json"];
        full.extend_from_slice(args);
        let out = self.spawn(&full);
        let value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
            panic!(
                "`openwhisper {}`: stdout is not JSON ({e}):\n{}\nstderr:\n{}",
                full.join(" "),
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            )
        });
        (out.status.code(), value)
    }

    /// Run a command that must fail; returns its stderr.
    fn fails(&self, args: &[&str]) -> String {
        let out = self.spawn(args);
        assert!(!out.status.success(), "`openwhisper {}` should have failed", args.join(" "));
        String::from_utf8_lossy(&out.stderr).to_string()
    }

    /// The decoy config folder has nothing in it: no command wrote
    /// where `OPENWHISPER_CONFIG_DIR` points.
    fn decoy_is_untouched(&self) {
        let left: Vec<_> = std::fs::read_dir(self.decoy()).unwrap().flatten().map(|e| e.file_name()).collect();
        assert!(left.is_empty(), "the runner wrote into the config folder of the environment: {left:?}");
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

const GROUPS: [&str; 5] = ["everyday", "spoken", "app-context", "modes", "failures"];

#[test]
fn list_shows_every_group_with_its_scenarios() {
    let scratch = Scratch::new("list");
    let text = scratch.ok(&["scenarios", "list"]);
    for group in GROUPS {
        assert!(text.contains(&format!("({group})\n")), "{group} is missing:\n{text}");
    }
    for name in [
        "everyday-weather-web-on",
        "everyday-weather-web-off",
        "everyday-what-day",
        "everyday-which-app",
        "everyday-what-computer",
        "everyday-context-off-day",
        "everyday-danish",
        "everyday-no-long-dashes",
        "spoken-english",
        "spoken-danish",
        "app-context-mapped-app",
        "app-context-unmapped-app",
        "app-context-screen-phrase",
        "app-context-agent-off",
        "modes-custom-mode",
        "modes-app-default-wins",
        "modes-dictation-clean",
        "failures-cli-missing",
        "failures-signed-out",
        "failures-endpoint-unreachable",
        "failures-timeout",
        "failures-cancel",
        "failures-no-screen-permission",
        "failures-protected-app",
        "failures-no-display",
    ] {
        assert!(text.contains(&format!("  {name}\n")), "{name} is missing:\n{text}");
    }
    assert!(text.contains("situation: ") && text.contains("expected:  "), "{text}");
    assert!(!text.contains(['\u{2014}', '\u{2013}']), "a long dash in the list");

    let (code, listed) = scratch.json(&["scenarios", "list"]);
    assert_eq!(code, Some(0));
    let listed = listed.as_array().expect("an array").clone();
    assert!(listed.len() >= 25, "{} scenarios", listed.len());
    for scenario in &listed {
        let name = scenario["name"].as_str().unwrap();
        let group = scenario["group"].as_str().unwrap();
        assert!(GROUPS.contains(&group), "{name} is in {group}");
        assert!(name.starts_with(&format!("{group}-")), "{name} is in {group}");
        assert!(scenario["limit_ms"].as_u64().unwrap() >= 1_000, "{name}");
    }
    // Listing makes nothing and asks nothing.
    scratch.decoy_is_untouched();
}

#[test]
fn the_offline_self_test_proves_every_check_without_a_model() {
    let scratch = Scratch::new("offline");
    let text = scratch.ok(&["scenarios", "run", "--offline"]);
    assert!(!text.contains("WRONG"), "{text}");
    let last = text.lines().last().unwrap_or_default();
    assert!(last.ends_with("checks behave (no model was asked)"), "{last}");

    let (code, record) = scratch.json(&["scenarios", "run", "--offline"]);
    assert_eq!(code, Some(0));
    assert_eq!(record["mode"], "offline");
    assert_eq!(record["passed"], record["total"]);
    let checks = record["checks"].as_array().unwrap();
    assert_eq!(checks.len() as u64, record["total"].as_u64().unwrap());
    // Every scenario of the list is proved: it accepts an ideal
    // outcome and rejects mistakes, the time limit among them.
    let (_, listed) = scratch.json(&["scenarios", "list"]);
    for scenario in listed.as_array().unwrap() {
        let name = scenario["name"].as_str().unwrap();
        let own: Vec<&str> =
            checks.iter().filter(|c| c["scenario"] == name).map(|c| c["what"].as_str().unwrap()).collect();
        assert!(own.contains(&"accepts an ideal outcome"), "{name}: {own:?}");
        assert!(own.contains(&"rejects: over the time limit"), "{name}: {own:?}");
        assert!(own.iter().filter(|what| what.starts_with("rejects: ")).count() >= 3, "{name}: {own:?}");
    }
    // Nothing was made for it: not a folder, not a settings file.
    scratch.decoy_is_untouched();
}

#[test]
fn only_narrows_a_run_to_a_group_or_a_prefix() {
    let scratch = Scratch::new("only");
    let (_, modes) = scratch.json(&["scenarios", "run", "--offline", "--only", "modes"]);
    let names = |record: &Value| -> Vec<String> {
        let mut names: Vec<String> =
            record["checks"].as_array().unwrap().iter().map(|c| c["scenario"].as_str().unwrap().to_string()).collect();
        names.dedup();
        names
    };
    assert_eq!(names(&modes), ["modes-custom-mode", "modes-app-default-wins", "modes-dictation-clean"]);
    let (_, spaced) = scratch.json(&["scenarios", "run", "--offline", "--only", "app context"]);
    assert!(names(&spaced).iter().all(|name| name.starts_with("app-context-")), "{:?}", names(&spaced));
    assert_eq!(names(&spaced).len(), 4);
    let (_, prefix) = scratch.json(&["scenarios", "run", "--offline", "--only", "everyday-weather"]);
    assert_eq!(names(&prefix), ["everyday-weather-web-on", "everyday-weather-web-off"]);

    let none = scratch.fails(&["scenarios", "run", "--offline", "--only", "no-such"]);
    assert!(none.contains("no scenario starts with \"no-such\""), "{none}");
    assert!(none.contains("everyday, spoken, app-context, modes, failures"), "{none}");
}

/// The real config folders of this computer, as names: the released
/// app's and the development build's. Nothing here opens them.
fn real_config_dirs() -> Vec<PathBuf> {
    // The same rule as `dirs::config_dir()` for the three platforms.
    let home = PathBuf::from(std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).expect("a home folder"));
    let base = if cfg!(target_os = "macos") {
        home.join("Library").join("Application Support")
    } else if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| home.join("AppData").join("Roaming"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()).map(PathBuf::from).unwrap_or_else(|| home.join(".config"))
    };
    vec![base.join("com.openwhisper.app"), base.join("com.openwhisper.app.dev")]
}

#[test]
fn a_real_config_folder_is_refused_before_anything_runs() {
    let scratch = Scratch::new("refuse");
    for real in real_config_dirs() {
        for dir in [real.clone(), real.join("history"), real.join("scratch").join("deeper")] {
            let dir = dir.display().to_string();
            // Also for a run that asks no model, and also when the
            // scenario asks no model and the CLI is a path to nothing.
            for args in [
                vec!["scenarios", "run", "--offline", "--config-dir", &dir],
                vec!["scenarios", "run", "--only", "failures-cli-missing", "--config-dir", &dir],
                vec!["scenarios", "run", "--config-dir", &dir, "--claude-path", "/made-up/no-claude-here"],
            ] {
                let out = scratch.spawn(&args);
                let (stdout, stderr) = (String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
                assert!(!out.status.success(), "`{}` ran", args.join(" "));
                assert!(stderr.contains("refusing to run: "), "{stderr}");
                assert!(stderr.contains("is a real OpenWhisper folder"), "{stderr}");
                // Not one scenario was started and nothing was announced.
                assert_eq!(stdout, "", "{stdout}");
                assert!(!stderr.contains("questions go to") && !stderr.contains("live in:"), "{stderr}");
            }
        }
    }
    // The folder that holds them is refused as well: the settings
    // would land next to the real ones.
    let around = real_config_dirs()[0].parent().unwrap().display().to_string();
    let why = scratch.fails(&["scenarios", "run", "--offline", "--config-dir", &around]);
    assert!(why.contains("refusing to run: "), "{why}");
}

#[test]
fn a_folder_with_files_in_it_is_refused() {
    let scratch = Scratch::new("not-empty");
    let dir = scratch.dir.join("somebody-elses");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("settings.json"), "{\"ask\":{\"enabled\":false}}").unwrap();
    let why = scratch.fails(&["scenarios", "run", "--only", "failures-cli-missing", "--config-dir", &dir.display().to_string()]);
    assert!(why.contains("refusing to run: ") && why.contains("has files in it"), "{why}");
    // What was there is as it was.
    assert_eq!(std::fs::read_to_string(dir.join("settings.json")).unwrap(), "{\"ask\":{\"enabled\":false}}");
}

#[test]
fn a_run_lives_in_its_scratch_folder_and_reports_what_was_seen() {
    let scratch = Scratch::new("run");
    let dir = scratch.dir.join("config");
    let dir_text = dir.display().to_string();
    // Two scenarios that ask no model: the CLI is not there, and the
    // endpoint is a closed port of this computer.
    let (code, record) = scratch.json(&["scenarios", "run", "--only", "failures-cli-missing", "--config-dir", &dir_text]);
    assert_eq!(code, Some(0), "{record}");
    assert_eq!(record["mode"], "live");
    assert_eq!((record["passed"].as_u64(), record["failed"].as_u64(), record["total"].as_u64()), (Some(1), Some(0), Some(1)));
    assert_eq!(record["stopped"], Value::Null);
    let row = &record["results"][0];
    assert_eq!(row["name"], "failures-cli-missing");
    assert_eq!(row["group"], "failures");
    assert_eq!(row["status"], "passed");
    assert!(row["elapsed_ms"].as_u64().unwrap() <= row["limit_ms"].as_u64().unwrap());
    let seen = &row["seen"];
    assert_eq!(seen["answer"], "");
    assert!(seen["error"].as_str().unwrap().contains("Claude Code is not installed"), "{seen}");
    assert_eq!(seen["captures"], 0);
    assert_eq!(seen["calls"], serde_json::json!([]));

    // The settings of the scenario are in the folder that was named,
    // as the settings panes would have written them, and the folder
    // is left in place.
    let settings: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("settings.json")).unwrap()).unwrap();
    assert_eq!(settings["ask"]["enabled"], true);
    assert!(settings["refine"]["claude_cli"]["path"].as_str().unwrap().ends_with("claude-is-not-here"), "{settings}");
    assert_eq!(settings["agent"]["enabled"], false);
    // No history and no database were made.
    assert!(!dir.join("history").exists() && !dir.join("openwhisper.db").exists());
    // And nothing went to the config folder of the environment.
    scratch.decoy_is_untouched();

    // A used folder is not taken a second time.
    let again = scratch.fails(&["scenarios", "run", "--only", "failures-cli-missing", "--config-dir", &dir_text]);
    assert!(again.contains("has files in it"), "{again}");
}

#[test]
fn a_folder_of_the_runs_own_is_removed_when_the_run_is_over() {
    let scratch = Scratch::new("own-folder");
    let out = scratch.spawn(&["scenarios", "run", "--only", "failures-endpoint"]);
    let (stdout, stderr) = (String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success(), "{stdout}\n{stderr}");
    assert!(stdout.contains("PASS  failures-endpoint-unreachable"), "{stdout}");
    assert!(stdout.contains("no answer: Could not reach 127.0.0.1:"), "{stdout}");
    assert!(stdout.trim_end().lines().last().unwrap().starts_with("1 passed, 0 failed, in "), "{stdout}");
    assert!(stderr.contains("no scenario of this run asks a model"), "{stderr}");
    // It says where it lives, and that place is gone afterwards.
    let line = stderr.lines().find(|l| l.contains("live in: ")).expect("where the run lives");
    let dir = PathBuf::from(line.split("live in: ").nth(1).unwrap().trim());
    assert!(dir.file_name().unwrap().to_string_lossy().starts_with("openwhisper-scenarios-"), "{}", dir.display());
    assert!(!dir.exists(), "{} is still there", dir.display());
    scratch.decoy_is_untouched();
}

#[cfg(unix)]
mod with_a_stand_in_cli {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// Answers `claude auth status` and `--version`, counts the
    /// questions it is sent, and streams ANSWER to each of them the
    /// way the real CLI does.
    const FAKE: &str = r#"#!/bin/sh
here="$(dirname "$0")"
if [ "$1" = "auth" ]; then echo '{"loggedIn": true, "authMethod": "claude.ai", "subscriptionType": "max"}'; exit 0; fi
if [ "$1" = "--version" ]; then echo '9.9.9 (Fake Claude)'; exit 0; fi
while read -r line; do
  n=$(cat "$here/asked.txt" 2>/dev/null || echo 0); n=$((n+1)); echo "$n" > "$here/asked.txt"
  echo '{"type":"stream_event","event":{"type":"message_start"}}'
  echo '{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"ANSWER"}}}'
  echo '{"type":"result","subtype":"success","is_error":false,"result":"ANSWER","usage":{"input_tokens":2,"output_tokens":5}}'
done
"#;

    /// Write a stand-in CLI that answers every question with
    /// `answer`, and return its path.
    fn fake_cli(scratch: &Scratch, name: &str, answer: &str) -> String {
        script(scratch, name, &FAKE.replace("ANSWER", answer))
    }

    /// The stand-in with an account at its rate limit: every
    /// question is refused the way the real CLI reports a 429.
    fn fake_cli_at_its_limit(scratch: &Scratch) -> String {
        let limited = FAKE.replace(
            "  echo '{\"type\":\"stream_event\",\"event\":{\"type\":\"message_start\"}}'\n",
            "  echo '{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":true,\"result\":\"API Error: 429 rate_limit_error: \
this request would exceed your rate limit\"}'; continue\n",
        );
        assert!(limited.contains("429 rate_limit_error"), "the fake changed shape");
        script(scratch, "claude-limited", &limited)
    }

    fn script(scratch: &Scratch, name: &str, body: &str) -> String {
        let dir = scratch.dir.join(format!("bin-{name}"));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("claude");
        std::fs::write(&path, body).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path.display().to_string()
    }

    fn asked(scratch: &Scratch, name: &str) -> String {
        std::fs::read_to_string(scratch.dir.join(format!("bin-{name}")).join("asked.txt")).unwrap_or_default().trim().to_string()
    }

    #[test]
    fn a_right_answer_passes_and_a_wrong_one_fails_with_what_was_seen() {
        let scratch = Scratch::new("judged");
        let today = chrono::Local::now().format("%A").to_string();
        let right = fake_cli(&scratch, "right", &format!("Today is {today}."));
        let out = scratch.spawn(&["scenarios", "run", "--only", "everyday-what-day", "--claude-path", &right]);
        let (stdout, stderr) = (String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        assert!(out.status.success(), "{stdout}\n{stderr}");
        assert!(stdout.contains("PASS  everyday-what-day"), "{stdout}");
        assert!(stdout.contains(&format!("Today is {today}.")), "{stdout}");
        assert!(stdout.contains("1 passed, 0 failed, in "), "{stdout}");
        assert!(stderr.contains("questions go to: Anthropic (Claude), through the Claude Code app"), "{stderr}");
        assert!(stderr.contains("the only connection is a test fixture, and the screen is a stand-in"), "{stderr}");
        assert_eq!(asked(&scratch, "right"), "1");

        let wrong = fake_cli(&scratch, "wrong", "Today is Someday.");
        let out = scratch.spawn(&["scenarios", "run", "--only", "everyday-what-day", "--claude-path", &wrong]);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert_eq!(out.status.code(), Some(1), "{stdout}");
        assert!(stdout.contains("FAIL  everyday-what-day"), "{stdout}");
        assert!(stdout.contains("situation: They ask what day it is."), "{stdout}");
        assert!(stdout.contains("problem:   the answer does not say today's weekday"), "{stdout}");
        assert!(stdout.contains("answer:    Today is Someday."), "{stdout}");
        assert!(stdout.contains("0 passed, 1 failed, in "), "{stdout}");

        let (code, record) = scratch.json(&["scenarios", "run", "--only", "everyday-what-day", "--claude-path", &wrong]);
        assert_eq!(code, Some(1));
        let row = &record["results"][0];
        assert_eq!(row["status"], "failed");
        assert_eq!(row["seen"]["answer"], "Today is Someday.");
        assert_eq!(row["seen"]["question"], "what day is it today");
        assert!(row["seen"]["facts"].as_array().unwrap().contains(&Value::String(today)), "{row}");
        assert_eq!((record["backend"].as_str(), record["model"].as_str(), record["effort"].as_str()), (Some("claude-cli"), Some("sonnet"), Some("low")));
        scratch.decoy_is_untouched();
    }

    #[test]
    fn the_facts_about_the_computer_reach_the_model_only_while_they_are_switched_on() {
        let scratch = Scratch::new("context");
        // A stand-in that repeats the name of the app when it was told
        // one, and says that it cannot know otherwise.
        let told = FAKE.replace(
            "while read -r line; do\n",
            "prev=\"\"; for a in \"$@\"; do if [ \"$prev\" = \"--system-prompt-file\" ]; then prompt=\"$a\"; fi; prev=\"$a\"; done\n\
if grep -q 'App in front: Harbor Notes' \"$prompt\"; then said='You are in Harbor Notes.'; else said='I cannot see which app you are in.'; fi\n\
while read -r line; do\n",
        )
        .replace("ANSWER", "'\"$said\"'");
        let cli = script(&scratch, "told", &told);
        for (name, answer) in [
            ("everyday-which-app", "You are in Harbor Notes."),
            ("everyday-context-off-app", "I cannot see which app you are in."),
        ] {
            let (code, record) = scratch.json(&["scenarios", "run", "--only", name, "--claude-path", &cli]);
            let row = &record["results"][0];
            assert_eq!(row["seen"]["answer"], answer, "{name}: {row}");
            assert_eq!((code, row["status"].as_str()), (Some(0), Some("passed")), "{name}: {row}");
        }
    }

    #[test]
    fn a_run_stops_at_a_rate_limit_instead_of_counting_failures() {
        let scratch = Scratch::new("limit");
        let limited = fake_cli_at_its_limit(&scratch);
        let out = scratch.spawn(&["scenarios", "run", "--only", "everyday", "--claude-path", &limited]);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert_eq!(out.status.code(), Some(2), "{stdout}");
        // The first scenario was refused: it is not a failed one, and
        // not one more question was sent.
        assert!(stdout.contains("STOP  everyday-weather-web-on"), "{stdout}");
        assert!(!stdout.contains("FAIL") && !stdout.contains("PASS"), "{stdout}");
        assert!(stdout.contains("----  everyday-what-day"), "{stdout}");
        assert!(stdout.contains("0 passed, 0 failed, 10 not run, in "), "{stdout}");
        assert!(stdout.contains("stopped: the backend reported a spend or rate limit at \"everyday-weather-web-on\""), "{stdout}");
        assert!(stdout.contains("10 scenarios were not run, nothing more was sent."), "{stdout}");
        assert_eq!(asked(&scratch, "claude-limited"), "1");

        let (code, record) = scratch.json(&["scenarios", "run", "--only", "everyday", "--claude-path", &limited]);
        assert_eq!(code, Some(2));
        assert_eq!(record["stopped"]["scenario"], "everyday-weather-web-on");
        assert_eq!(record["stopped"]["not_run"], 10);
        assert_eq!((record["passed"].as_u64(), record["failed"].as_u64(), record["not_run"].as_u64()), (Some(0), Some(0), Some(10)));
        assert_eq!(record["results"][0]["status"], "stopped");
        assert_eq!(record["results"][1]["status"], "not-run");
        assert_eq!(asked(&scratch, "claude-limited"), "2");
    }

    #[test]
    fn a_backend_that_is_not_ready_ends_the_run_before_the_first_question() {
        let scratch = Scratch::new("not-ready");
        let why = scratch.fails(&["scenarios", "run", "--only", "everyday-what-day", "--claude-path", "/made-up/no-claude-here"]);
        assert!(why.contains("backend claude-cli is not ready: "), "{why}");
        // The scenarios that ask no model run all the same.
        let text = scratch.ok(&["scenarios", "run", "--only", "failures-signed-out", "--claude-path", "/made-up/no-claude-here"]);
        assert!(text.contains("PASS  failures-signed-out"), "{text}");
        assert!(text.contains("Claude Code is installed but not signed in."), "{text}");
    }

    #[test]
    fn a_model_that_never_answers_ends_as_a_timeout_and_its_process_is_gone() {
        let scratch = Scratch::new("timeout");
        let (code, record) = scratch.json(&["scenarios", "run", "--only", "failures-timeout"]);
        let row = &record["results"][0];
        assert_eq!((code, row["status"].as_str()), (Some(0), Some("passed")), "{row}");
        assert_eq!(row["seen"]["error"], "no answer in time");
        assert_eq!(row["seen"]["process_gone"], true);
        assert!(row["elapsed_ms"].as_u64().unwrap() >= 4_500, "{row}");
    }
}
