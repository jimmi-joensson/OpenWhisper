//! End-to-end checks for `openwhisper ask` (TASK-97). Spawns the bin,
//! like the modes tests, and reads its output.
//!
//! Every invocation gets `OPENWHISPER_CONFIG_DIR` pointed at a folder
//! of its own under cargo's test tmp dir, so nothing here can read or
//! write a real `settings.json`. No command in this file calls a
//! model: where an answer is needed, `--claude-path` points at a shell
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
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("ask-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch config dir");
        Self { dir }
    }

    fn spawn(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_openwhisper"))
            .args(args)
            .env("OPENWHISPER_CONFIG_DIR", &self.dir)
            // The API key is read from this variable before the OS
            // keychain is looked at, so no test here touches (or
            // makes macOS ask about) a real keychain entry.
            .env("OPENWHISPER_REFINE_API_KEY", "not-a-real-key")
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

#[test]
fn a_fresh_config_has_ask_off_and_says_where_questions_would_go() {
    let cfg = Config::new("fresh");
    let status = cfg.json(&["ask", "status", "--claude-path", "/nonexistent/claude"]);
    assert_eq!(status["settings"]["enabled"], false);
    assert_eq!(status["settings"]["web_search"], false);
    assert_eq!(status["settings"]["timeout_ms"], 60_000);
    assert_eq!(status["model"], "sonnet");
    assert_eq!(status["effort"], "low");
    assert_eq!(status["web_search"], false);
    assert_eq!(status["timeout_ms"], 60_000);
    assert_eq!(status["backend"]["backend"], "claude-cli");
    assert_eq!(status["backend"]["state"], "not-installed");
    assert_eq!(status["backend"]["ready"], false);
    let destination = status["destination"].as_str().expect("destination");
    assert!(destination.contains("Anthropic (Claude)"), "{destination}");
    assert_eq!(status["backend"]["destination"], status["destination"]);
    // Reading the status wrote nothing.
    assert!(!cfg.dir.join("settings.json").exists());

    // For people: the same, as lines.
    let out = cfg.spawn(&["ask", "status", "--claude-path", "/nonexistent/claude"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("enabled           false"), "{stdout}");
    assert!(stdout.contains("question goes to  Anthropic (Claude)"), "{stdout}");
    assert!(stdout.contains("tools             none\n"), "{stdout}");

    // The other backend: no web search to switch on, and it says so.
    let status = cfg.json(&[
        "ask",
        "status",
        "--backend",
        "openai-compatible",
        "--base-url",
        "http://localhost:11434/v1",
        "--model",
        "local-model",
    ]);
    assert_eq!(status["model"], "local-model");
    assert_eq!(status["effort"], "");
    assert_eq!(status["destination"], "the server at localhost:11434");
    assert_eq!(status["backend"]["state"], "ready");
}

#[test]
fn config_changes_the_ask_block_and_nothing_else() {
    let cfg = Config::new("config");
    // Something another command wrote first.
    cfg.json(&["refine", "config", "--instructions", "Fix commas."]);

    let shown = cfg.json(&["ask", "config"]);
    assert_eq!(shown["enabled"], false);
    assert_eq!(shown["model"], Value::Null);
    assert_eq!(shown["effort"], Value::Null);
    assert_eq!(cfg.settings_file().get("ask"), None, "printing the block wrote it");

    let set = cfg.json(&[
        "ask",
        "config",
        "--enabled",
        "true",
        "--web-search",
        "on",
        "--instructions",
        "  Answer in Danish. ",
        "--model",
        "opus",
        "--effort",
        "high",
        "--timeout-ms",
        "1",
    ]);
    assert_eq!(set["enabled"], true);
    assert_eq!(set["web_search"], true);
    assert_eq!(set["instructions"], "Answer in Danish.");
    assert_eq!(set["model"], "opus");
    assert_eq!(set["effort"], "high");
    assert_eq!(set["timeout_ms"], 5_000, "clamped to the shortest timeout");

    let file = cfg.settings_file();
    assert_eq!(file["ask"], set);
    assert_eq!(file["refine"]["instructions"], "Fix commas.");
    // Refine's own model is not Ask's.
    assert_eq!(file["refine"]["claude_cli"]["model"], "sonnet");

    // What an ask would run with now: web search on doubles the timeout.
    let status = cfg.json(&["ask", "status", "--claude-path", "/nonexistent/claude"]);
    assert_eq!((status["model"].as_str(), status["effort"].as_str()), (Some("opus"), Some("high")));
    assert_eq!(status["web_search"], true);
    assert_eq!(status["timeout_ms"], 10_000);

    let cleared = cfg.json(&["ask", "config", "--clear-model", "--clear-effort", "--web-search", "false"]);
    assert_eq!(cleared["model"], Value::Null);
    assert_eq!(cleared["effort"], Value::Null);
    assert_eq!(cleared["web_search"], false);
    assert_eq!(cleared["enabled"], true, "the other fields stay");
    assert_eq!(cleared["instructions"], "Answer in Danish.");
    assert_eq!(cfg.json(&["ask", "config", "--instructions", ""])["instructions"], "");

    // What cannot be done says why, and exits non-zero.
    assert!(cfg.fails(&["ask", "config", "--effort", "turbo"]).contains("turbo"));
    assert!(cfg.fails(&["ask", "config", "--enabled", "maybe"]).contains("maybe"));
    assert!(cfg.fails(&["ask", "config", "--model", "opus", "--clear-model"]).contains("cannot be used with"));
    assert_eq!(cfg.json(&["ask", "config"])["effort"], Value::Null, "a refused change wrote nothing");

    // A later save of another block keeps the ask block.
    cfg.json(&["refine", "config", "--min-words", "3"]);
    assert_eq!(cfg.settings_file()["ask"]["enabled"], true);
}

#[test]
fn run_without_a_question_fails_before_anything_is_sent() {
    let cfg = Config::new("no-question");
    // The test's stdin is closed, so there is nothing to read either.
    let err = cfg.fails(&["ask", "run", "--claude-path", "/nonexistent/claude"]);
    assert!(err.contains("no question given"), "{err}");
    let err = cfg.fails(&["ask", "run", "   ", "--claude-path", "/nonexistent/claude"]);
    assert!(err.contains("no question given"), "{err}");
    assert!(!err.contains("question goes to"), "{err}");
    assert!(cfg.fails(&["ask", "run", "hi", "--effort", "turbo"]).contains("turbo"));
}

#[test]
fn run_reports_a_missing_backend_and_exits_non_zero() {
    let cfg = Config::new("missing-backend");
    let out = cfg.spawn(&["ask", "run", "what is two plus two", "--claude-path", "/nonexistent/claude"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("question goes to: Anthropic (Claude)"), "{stderr}");
    assert!(stderr.contains("no answer: Claude Code is not installed"), "{stderr}");
    assert!(out.stdout.is_empty());

    // The JSON record says the same.
    let out = cfg.spawn(&["--json", "ask", "run", "what is two plus two", "--claude-path", "/nonexistent/claude"]);
    assert!(!out.status.success());
    let record: Value = serde_json::from_slice(&out.stdout).expect("a JSON record");
    assert_eq!(record["question"], "what is two plus two");
    assert_eq!(record["answer"], Value::Null);
    assert!(record["error"].as_str().is_some_and(|e| e.contains("not installed")), "{record}");
    assert_eq!(record["backend"], "claude-cli");
    assert_eq!(record["model"], "sonnet");
}

/// The commands that need an answer, against a script standing in for
/// the Claude CLI.
#[cfg(unix)]
mod with_a_fake_cli {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// Answers `claude auth status` and `--version`, records its
    /// arguments, the system prompt and each question, and streams a
    /// two-piece answer per question the way the real CLI does.
    const FAKE: &str = r#"#!/bin/sh
here="$(dirname "$0")"
if [ "$1" = "auth" ]; then echo '{"loggedIn": true, "authMethod": "claude.ai", "subscriptionType": "max"}'; exit 0; fi
if [ "$1" = "--version" ]; then echo '9.9.9 (Fake Claude)'; exit 0; fi
echo "$@" > "$here/args.txt"
prev=""; for a in "$@"; do if [ "$prev" = "--system-prompt-file" ]; then cp "$a" "$here/system.txt"; fi; prev="$a"; done
while read -r line; do
  printf '%s\n' "$line" > "$here/stdin.txt"
  echo '{"type":"stream_event","event":{"type":"message_start"}}'
  echo '{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"easy"}}}'
  echo '{"type":"stream_event","event":{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"The capital of Norway "}}}'
  echo '{"type":"stream_event","event":{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"is Oslo."}}}'
  echo '{"type":"result","subtype":"success","is_error":false,"result":"The capital of Norway is Oslo.","total_cost_usd":0.0025,"usage":{"input_tokens":2,"cache_creation_input_tokens":560,"cache_read_input_tokens":40,"output_tokens":11}}'
done
"#;

    /// Write the fake next to the scratch config and return its path.
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

    #[test]
    fn run_streams_the_answer_to_stdout_and_the_rest_to_stderr() {
        let cfg = Config::new("run");
        let claude = fake_cli(&cfg);
        // Ask is off in this fresh config: an explicit run is the
        // opt-in, and it says where the question goes.
        let out = cfg.spawn(&["ask", "run", "what is the capital of norway", "--claude-path", &claude]);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "{stderr}");
        assert_eq!(String::from_utf8_lossy(&out.stdout), "The capital of Norway is Oslo.\n");
        assert!(stderr.contains("question goes to: Anthropic (Claude), through the Claude Code app"), "{stderr}");
        assert!(stderr.contains("first text after "), "{stderr}");
        assert!(stderr.contains("done in "), "{stderr}");
        assert!(stderr.contains("(claude-cli, sonnet, low effort)"), "{stderr}");
        assert!(stderr.contains("tokens: 602 in (40 cached), 11 out"), "{stderr}");

        // The question went in fenced, on its own; the instructions
        // went in through the prompt file.
        let sent: Value = serde_json::from_str(&recorded(&cfg, "stdin.txt")).expect("the user line");
        assert_eq!(sent["message"]["content"][0]["text"], "<question>\nwhat is the capital of norway\n</question>");
        let system = recorded(&cfg, "system.txt");
        assert!(system.contains("a speech recognizer wrote it down"), "{system}");
        assert!(system.contains("You cannot look anything up"), "{system}");
        // No tools, and the permission mode pinned.
        let args = recorded(&cfg, "args.txt");
        assert!(args.contains("--permission-mode default"), "{args}");
        assert!(args.contains("--no-session-persistence"), "{args}");
        assert!(args.contains("--model sonnet --effort low"), "{args}");
        assert!(!args.contains("WebSearch") && !args.contains("--allowedTools"), "{args}");
        // Nothing was saved by running.
        assert!(!cfg.dir.join("settings.json").exists());
    }

    #[test]
    fn run_takes_the_question_from_stdin_and_overrides_without_saving() {
        use std::io::Write;
        use std::process::Stdio;
        let cfg = Config::new("stdin");
        let claude = fake_cli(&cfg);
        let mut child = Command::new(env!("CARGO_BIN_EXE_openwhisper"))
            .args([
                "ask",
                "run",
                "--no-stream",
                "--web-search",
                "--model",
                "opus",
                "--effort",
                "medium",
                "--instructions",
                "Answer in one sentence.",
                "--claude-path",
                &claude,
            ])
            .env("OPENWHISPER_CONFIG_DIR", &cfg.dir)
            .env("OPENWHISPER_REFINE_API_KEY", "not-a-real-key")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn openwhisper");
        child.stdin.take().unwrap().write_all(b"what is the capital of norway\n").unwrap();
        let out = child.wait_with_output().unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "{stderr}");
        assert_eq!(String::from_utf8_lossy(&out.stdout), "The capital of Norway is Oslo.\n");
        assert!(stderr.contains("(claude-cli, opus, medium effort)"), "{stderr}");

        // Web search is exactly the two WebSearch flags.
        let args = recorded(&cfg, "args.txt");
        assert!(args.contains("--tools WebSearch --allowedTools WebSearch --strict-mcp-config"), "{args}");
        assert!(args.contains("--model opus --effort medium"), "{args}");
        assert!(args.contains("--permission-mode default"), "{args}");
        let system = recorded(&cfg, "system.txt");
        assert!(system.contains("You can search the web"), "{system}");
        assert!(system.ends_with("Answer in one sentence."), "{system}");
        assert!(!cfg.dir.join("settings.json").exists(), "an override was saved");
    }

    #[test]
    fn run_as_json_prints_one_record() {
        let cfg = Config::new("json");
        let claude = fake_cli(&cfg);
        let record = cfg.json(&["ask", "run", "what is the capital of norway", "--claude-path", &claude]);
        assert_eq!(record["question"], "what is the capital of norway");
        assert_eq!(record["answer"], "The capital of Norway is Oslo.");
        assert_eq!(record["error"], Value::Null);
        assert!(record["first_text_ms"].is_u64(), "{record}");
        assert!(record["elapsed_ms"].as_u64() >= record["first_text_ms"].as_u64(), "{record}");
        assert_eq!(record["usage"]["tokens_in"], 602);
        assert_eq!(record["usage"]["tokens_out"], 11);
        assert_eq!(record["backend"], "claude-cli");
        assert_eq!(record["model"], "sonnet");
        assert_eq!(record["effort"], "low");
        assert_eq!(record["web_search"], false);
        assert!(record["destination"].as_str().is_some_and(|d| d.contains("Anthropic")), "{record}");
    }

    #[test]
    fn run_uses_the_dictionary_like_a_dictation_does() {
        let cfg = Config::new("dictionary");
        let claude = fake_cli(&cfg);
        std::fs::write(
            cfg.dir.join("dictionary.json"),
            r#"{"terms":[{"term":"Brennholt","heard_as":["bren holt"]}]}"#,
        )
        .unwrap();
        cfg.json(&["ask", "run", "who runs bren holt", "--claude-path", &claude]);
        // The rule pass fixed the listed mis-hearing before the
        // question left, and the prompt names the term.
        let sent: Value = serde_json::from_str(&recorded(&cfg, "stdin.txt")).unwrap();
        let question = sent["message"]["content"][0]["text"].as_str().unwrap();
        assert!(question.contains("Brennholt") && !question.contains("bren holt"), "{question}");
        assert!(recorded(&cfg, "system.txt").contains("Brennholt [misheard as: bren holt]"));

        // --skip-rules sends the words as given.
        cfg.json(&["ask", "run", "who runs bren holt", "--skip-rules", "--claude-path", &claude]);
        let sent: Value = serde_json::from_str(&recorded(&cfg, "stdin.txt")).unwrap();
        assert_eq!(sent["message"]["content"][0]["text"], "<question>\nwho runs bren holt\n</question>");
    }

    #[test]
    fn eval_runs_the_cases_it_is_asked_for_and_reports_latency() {
        let cfg = Config::new("eval");
        let claude = fake_cli(&cfg);
        // The fake always answers "Oslo": right for the fact case.
        let report = cfg.json(&["ask", "eval", "--only", "fact", "--claude-path", &claude]);
        assert_eq!((report["passed"].as_u64(), report["total"].as_u64()), (Some(1), Some(1)));
        assert_eq!(report["results"][0]["name"], "fact-en");
        assert_eq!(report["results"][0]["passed"], true);
        assert_eq!(report["results"][0]["answer"], "The capital of Norway is Oslo.");
        assert_eq!(report["latency_ms"]["first_text"]["answers"], 1);
        assert_eq!(report["latency_ms"]["total"]["answers"], 1);
        assert_eq!(report["model"], "sonnet");
        // The eval prompt carries the eval dictionary, not the user's.
        assert!(recorded(&cfg, "system.txt").contains("Brennholt [misheard as: bren holt, brain holt]"));

        // And wrong for the calculation: the run fails, with the reason.
        let out = cfg.spawn(&["ask", "eval", "--only", "calc", "--claude-path", &claude]);
        assert!(!out.status.success());
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains("FAIL  calculation"), "{stdout}");
        assert!(stdout.contains("problem: has none of [\"391\"]"), "{stdout}");
        assert!(stdout.contains("0/1 passed"), "{stdout}");
        assert!(stdout.contains("first text min"), "{stdout}");

        assert!(cfg.fails(&["ask", "eval", "--only", "nope", "--claude-path", &claude]).contains("no eval case starts with"));
        // Without a backend that is ready, no case is run.
        assert!(cfg.fails(&["ask", "eval", "--claude-path", "/nonexistent/claude"]).contains("is not ready"));
    }
}
