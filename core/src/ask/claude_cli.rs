//! Ask backend: Claude, through the Claude Code CLI the user installed
//! and signed in themselves.
//!
//! The same binary, lookup, `PATH` handling and lean flags as Refine's
//! backend ([`crate::refine::claude_cli`], which also explains why the
//! flags are what they are and what differs on Windows). This file
//! only adds what an ask needs on top:
//!
//! - **Streaming.** `--include-partial-messages` makes the CLI print
//!   the answer as it is written (`stream_event` lines carrying
//!   `text_delta`s) instead of one `result` line at the end. Thinking
//!   deltas can come first and are skipped.
//! - **No tools, pinned.** `--tools ""` as in Refine, and always
//!   `--permission-mode default`: the CLI's own default mode is a
//!   per-machine setting and can be a looser one. With web search on,
//!   the tool list is exactly `WebSearch`. If the model is seen
//!   starting any other tool, the turn fails and the process is
//!   killed; with these flags that cannot happen, so if it does, the
//!   flags no longer mean what this file assumes.
//! - **A conversation.** stdin stays open after the first question,
//!   so a later [`AskBackend::send`] continues in the same process
//!   with the context kept (TASK-98 sends the screen that way). One
//!   user line per turn; the turn ends at the CLI's `result` line.
//! - **Cancel.** The reader thread hands lines over a channel and the
//!   turn polls it, so a cancel or the timeout kills the process
//!   within a few milliseconds even while the model is silent (a web
//!   search prints nothing for 15 seconds or more). The question is
//!   written to stdin from a thread of its own for the same reason: a
//!   large one (a screenshot) can fill the pipe before the CLI reads,
//!   and a write that waits must not keep the turn from being
//!   cancelled or timing out. On Windows the kill takes the whole
//!   process tree, see [`refine_cli::end_process`].
//!
//! Measured with Claude Code 2.1.322, Sonnet at low effort
//! (2026-11-10): cold, the first text is back after 1.3 to 2.3 s;
//! with the process spawned at record start, after 0.5 to 0.7 s.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use base64::Engine as _;
use serde_json::{Value, json};

use super::{AskBackend, AskError, AskEvent, AskInput, AskReply, AskSetup, CancelFlag, POLL, Turn};
use crate::refine::claude_cli as refine_cli;
use crate::refine::{CallUsage, PlanWindows};
use crate::settings::{ClaudeCliSettings, RefineEffort};

/// Backend id: the same backend as Refine's.
pub const ID: &str = refine_cli::ID;

/// The one tool an ask may have, and only with web search on.
const WEB_SEARCH_TOOL: &str = "WebSearch";

/// The Claude Code CLI as an [`AskBackend`].
pub struct ClaudeCliAsk {
    /// Only [`ClaudeCliSettings::path`] is used: the model and effort
    /// of an ask come with the [`AskSetup`].
    settings: ClaudeCliSettings,
    process: Option<Process>,
}

impl ClaudeCliAsk {
    pub fn new(settings: ClaudeCliSettings) -> Self {
        Self { settings, process: None }
    }

    fn spawn(&self, setup: &AskSetup) -> Result<Process, AskError> {
        let bin = refine_cli::find_binary(&self.settings)
            .ok_or_else(|| AskError::Unavailable(refine_cli::NOT_INSTALLED.to_string()))?;
        Process::spawn(&bin, setup)
    }
}

impl AskBackend for ClaudeCliAsk {
    fn id(&self) -> &'static str {
        ID
    }

    fn warm(&mut self, setup: &AskSetup) {
        // A failed spawn is reported by `send`, which tries again.
        self.process = self.spawn(setup).ok();
    }

    fn send(
        &mut self,
        setup: &AskSetup,
        input: &AskInput,
        sink: &mut dyn FnMut(AskEvent),
        cancel: &CancelFlag,
        timeout: Duration,
    ) -> Result<AskReply, AskError> {
        let started = Instant::now();
        let mut warm_lead = None;
        let mut process = match self.process.take() {
            // The setup is fixed at spawn. If it changed since
            // (dictionary edited mid-recording), start over.
            Some(p) if p.setup == *setup => {
                if p.turns == 0 {
                    warm_lead = Some(p.spawned_at.elapsed());
                }
                p
            }
            _ => self.spawn(setup)?,
        };
        let reply = process.turn(input, sink, cancel, started, timeout, warm_lead)?;
        // Kept for a further turn. On any error above the process was
        // dropped instead, which kills it.
        self.process = Some(process);
        Ok(reply)
    }
}

/// Arguments for one headless conversation: Refine's lean flags
/// ([`refine_cli::spawn_args`]) with streaming, the permission mode
/// pinned and, with web search on, that one tool.
///
/// `--tools` and `--allowedTools` take a list, so each is followed by
/// another `--flag`, never by a bare value that could be read as one
/// more tool.
pub fn spawn_args(setup: &AskSetup, system_prompt_file: &Path) -> Vec<String> {
    // The Claude CLI has no "minimal" level.
    let effort = match setup.effort {
        None | Some(RefineEffort::Minimal) => RefineEffort::Low,
        Some(effort) => effort,
    };
    let mut args = refine_cli::spawn_args(&setup.model, effort, system_prompt_file);
    if setup.web_search
        && let Some(tools) = args.iter().position(|a| a == "--tools")
        && tools + 1 < args.len()
    {
        args[tools + 1] = WEB_SEARCH_TOOL.to_string();
        args.splice(
            tools + 2..tools + 2,
            ["--allowedTools".to_string(), WEB_SEARCH_TOOL.to_string()],
        );
    }
    args.extend(
        ["--include-partial-messages", "--permission-mode", "default"]
            .iter()
            .map(|s| (*s).to_string()),
    );
    args
}

/// The user line of one turn, in the CLI's stream-json input format:
/// images first (as base64 blocks), then the text.
pub fn user_line(input: &AskInput) -> String {
    let mut content: Vec<Value> = input
        .images
        .iter()
        .map(|image| {
            json!({
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": image.media_type,
                    "data": base64::engine::general_purpose::STANDARD.encode(&image.data),
                },
            })
        })
        .collect();
    content.push(json!({ "type": "text", "text": input.text }));
    json!({ "type": "user", "message": { "role": "user", "content": content } }).to_string()
}

/// One spawned CLI process: one conversation.
struct Process {
    child: Child,
    stdin: Option<ChildStdin>,
    /// stdout, line by line. Disconnects when the process ends.
    lines: Receiver<String>,
    /// What the process wrote to stderr, complete once it has ended.
    errors: Arc<Mutex<String>>,
    /// The setup this process was started with.
    setup: AskSetup,
    /// Temp file holding the system prompt; deleted on drop.
    prompt_file: PathBuf,
    spawned_at: Instant,
    /// Turns answered so far.
    turns: u32,
    /// A question was sent and its result has not come back: the
    /// model may be working on it.
    mid_turn: bool,
}

impl Process {
    fn spawn(bin: &Path, setup: &AskSetup) -> Result<Self, AskError> {
        let prompt_file = refine_cli::write_prompt_file("Ask", &setup.system)?;
        let mut cmd = refine_cli::command(bin);
        cmd.args(spawn_args(setup, &prompt_file))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) => {
                let _ = fs::remove_file(&prompt_file);
                return Err(AskError::Unavailable(format!(
                    "Could not start Claude Code ({}): {e}",
                    bin.display()
                )));
            }
        };
        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let errors = Arc::new(Mutex::new(String::new()));
        if let Some(mut stderr) = stderr {
            let errors = errors.clone();
            thread::spawn(move || {
                let mut buf = String::new();
                let _ = stderr.read_to_string(&mut buf);
                if let Ok(mut e) = errors.lock() {
                    *e = buf;
                }
            });
        }

        let (tx, lines) = mpsc::channel();
        if let Some(stdout) = stdout {
            thread::spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break };
                    if tx.send(line).is_err() {
                        // Nobody is listening any more.
                        return;
                    }
                }
                // The process ended. Give the stderr reader a moment
                // before the channel closes and the turn asks what it
                // said.
                thread::sleep(Duration::from_millis(100));
            });
        }

        Ok(Self {
            child,
            stdin,
            lines,
            errors,
            setup: setup.clone(),
            prompt_file,
            spawned_at: Instant::now(),
            turns: 0,
            mid_turn: false,
        })
    }

    /// Write one user line and read until that turn's `result` line.
    fn turn(
        &mut self,
        input: &AskInput,
        sink: &mut dyn FnMut(AskEvent),
        cancel: &CancelFlag,
        started: Instant,
        timeout: Duration,
        warm_lead: Option<Duration>,
    ) -> Result<AskReply, AskError> {
        let Some(mut stdin) = self.stdin.take() else {
            return Err(AskError::Backend("Claude Code has no input channel".into()));
        };
        // Written on a thread of its own: the line can be large, and
        // a CLI that is not reading yet lets the write wait. This
        // thread keeps its eye on the cancel flag and the clock
        // meanwhile. When the turn is given up, the process is killed
        // (on drop), which ends a write that is still waiting.
        let line = user_line(input);
        let (wrote_tx, wrote) = mpsc::channel();
        thread::spawn(move || {
            let result = stdin
                .write_all(line.as_bytes())
                .and_then(|()| stdin.write_all(b"\n"))
                .and_then(|()| stdin.flush());
            // stdin goes back for a further turn. When nobody waits
            // for it any more, it closes here.
            let _ = wrote_tx.send((result, stdin));
        });
        self.mid_turn = true;
        let mut writing = true;

        let deadline = started + timeout;
        let mut turn = Turn::new(sink, started);
        let mut plan: Option<PlanWindows> = None;
        loop {
            if cancel.is_cancelled() {
                return Err(AskError::Cancelled);
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(AskError::Timeout);
            }
            if writing {
                match wrote.try_recv() {
                    Ok((Ok(()), stdin)) => {
                        self.stdin = Some(stdin);
                        writing = false;
                    }
                    Ok((Err(_), _)) | Err(TryRecvError::Disconnected) => {
                        // The process is already gone; wait for its
                        // last words.
                        return Err(match self.lines.recv_timeout(Duration::from_millis(500)) {
                            Err(RecvTimeoutError::Disconnected) => self.exit_error(),
                            _ => AskError::Backend("Claude Code exited before it was asked".into()),
                        });
                    }
                    Err(TryRecvError::Empty) => {}
                }
            }
            let line = match self.lines.recv_timeout(POLL.min(deadline - now)) {
                Ok(line) => line,
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => return Err(self.exit_error()),
            };
            match read_line(&line, self.setup.web_search)? {
                Line::Text(delta) => turn.text(&delta),
                Line::NewMessage => turn.new_message(),
                Line::Searching(query) => turn.event(AskEvent::Searching { query }),
                Line::SearchDone => turn.event(AskEvent::SearchDone),
                Line::Plan(windows) => plan = Some(windows),
                Line::Result(text) => {
                    // A CLI that did not stream still answers.
                    if !turn.has_text() {
                        turn.text(&text);
                    }
                    let usage: Option<CallUsage> = refine_cli::parse_usage(&line).map(|mut call| {
                        call.plan = plan.take();
                        call
                    });
                    self.turns += 1;
                    self.mid_turn = false;
                    return Ok(turn.finish(usage, warm_lead, ""));
                }
                Line::Other => {}
            }
        }
    }

    fn exit_error(&self) -> AskError {
        let said = self.errors.lock().map(|e| e.clone()).unwrap_or_default();
        refine_cli::exit_error(&said).into()
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        // Close stdin first: an idle CLI exits on its own once its
        // input is closed. One that is working on a turn finishes the
        // turn first, so it is killed, and on Windows everything
        // under it too: behind a `claude.cmd` shim the CLI is a
        // grandchild that a plain kill does not reach.
        drop(self.stdin.take());
        refine_cli::end_process(&mut self.child, self.mid_turn);
        let _ = fs::remove_file(&self.prompt_file);
    }
}

/// What one line of the CLI's output means for an ask.
#[derive(Debug, PartialEq)]
enum Line {
    /// A piece of the answer.
    Text(String),
    /// The model started a new message (after a search).
    NewMessage,
    /// The model searches the web for this.
    Searching(String),
    /// The search came back.
    SearchDone,
    /// The plan's usage windows, for the usage of this turn.
    Plan(PlanWindows),
    /// The turn is over; the whole answer as the CLI reports it.
    Result(String),
    /// Nothing an ask acts on (init, thinking, block boundaries).
    Other,
}

/// Read one stdout line. An error ends the turn: the CLI reported a
/// failure, or the model reached for a tool an ask may not use.
fn read_line(line: &str, web_search: bool) -> Result<Line, AskError> {
    let Ok(v) = serde_json::from_str::<Value>(line.trim()) else {
        return Ok(Line::Other);
    };
    match v["type"].as_str() {
        Some("stream_event") => {
            let event = &v["event"];
            match event["type"].as_str() {
                Some("message_start") => Ok(Line::NewMessage),
                Some("content_block_delta") if event["delta"]["type"].as_str() == Some("text_delta") => {
                    Ok(Line::Text(event["delta"]["text"].as_str().unwrap_or("").to_string()))
                }
                // Caught here, as the block starts, and not only on
                // the `assistant` line that follows it.
                Some("content_block_start") => {
                    check_tool(&event["content_block"], web_search)?;
                    Ok(Line::Other)
                }
                _ => Ok(Line::Other),
            }
        }
        // The finished message. Its text already arrived as deltas;
        // a tool call is complete here, with its input.
        Some("assistant") => {
            let mut searching = None;
            for block in v["message"]["content"].as_array().into_iter().flatten() {
                if check_tool(block, web_search)? {
                    searching = Some(block["input"]["query"].as_str().unwrap_or("").to_string());
                }
            }
            Ok(searching.map_or(Line::Other, Line::Searching))
        }
        // The tool's answer, fed back to the model.
        Some("user") => {
            let has_result = v["message"]["content"]
                .as_array()
                .is_some_and(|blocks| blocks.iter().any(|b| b["type"].as_str() == Some("tool_result")));
            Ok(if has_result { Line::SearchDone } else { Line::Other })
        }
        Some("rate_limit_event") => Ok(refine_cli::parse_rate_limit_line(line).map_or(Line::Other, Line::Plan)),
        Some("result") => match refine_cli::parse_result_line(line) {
            Some(Ok(text)) => Ok(Line::Result(text)),
            Some(Err(e)) => Err(e.into()),
            None => Ok(Line::Other),
        },
        _ => Ok(Line::Other),
    }
}

/// `Ok(true)` when `block` is the web search and web search is on,
/// `Ok(false)` when it is not a tool call at all. Any other tool is an
/// error: an ask may not use tools.
fn check_tool(block: &Value, web_search: bool) -> Result<bool, AskError> {
    let kind = block["type"].as_str().unwrap_or("");
    if !kind.ends_with("tool_use") {
        return Ok(false);
    }
    let name = block["name"].as_str().unwrap_or("unknown");
    if web_search && name == WEB_SEARCH_TOOL {
        Ok(true)
    } else {
        Err(AskError::Backend(format!(
            "The model tried to use a tool ({name}). An ask may not use tools, so it was stopped."
        )))
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::super::AskImage;
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    /// A fake `claude`: a shell script standing in for the real CLI.
    fn fake_cli(dir: &Path, body: &str) -> ClaudeCliSettings {
        let path = dir.join("claude");
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        ClaudeCliSettings { path: Some(path.display().to_string()), ..ClaudeCliSettings::default() }
    }

    /// Records its arguments and prompt file, then answers every line
    /// on stdin the way the real CLI streams: a thinking delta, two
    /// text deltas, the finished message, the plan windows, the
    /// result. The answer names the turn, so a second turn can be
    /// told from the first.
    const STREAM_OK: &str = r#"
echo "$@" > "$(dirname "$0")/args.txt"
prev=""; for a in "$@"; do if [ "$prev" = "--system-prompt-file" ]; then cp "$a" "$(dirname "$0")/system.txt"; fi; prev="$a"; done
n=0
while read -r line; do
  n=$((n+1))
  printf '%s\n' "$line" > "$(dirname "$0")/stdin-$n.txt"
  echo '{"type":"system","subtype":"init"}'
  echo '{"type":"stream_event","event":{"type":"message_start"}}'
  echo '{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}}'
  echo '{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"a capital, easy"}}}'
  echo '{"type":"stream_event","event":{"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}}'
  echo '{"type":"stream_event","event":{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"The capital of Norway "}}}'
  echo "{\"type\":\"stream_event\",\"event\":{\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"text_delta\",\"text\":\"is Oslo (turn $n).\"}}}"
  echo "{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"The capital of Norway is Oslo (turn $n).\"}]}}"
  echo '{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","unifiedWindows":{"five_hour":{"utilization":0.09,"resetsAt":1794325200},"seven_day":{"utilization":0.21,"resetsAt":1794646800}}}}'
  echo "{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"result\":\"The capital of Norway is Oslo (turn $n).\",\"total_cost_usd\":0.0025,\"usage\":{\"input_tokens\":2,\"cache_creation_input_tokens\":560,\"cache_read_input_tokens\":40,\"output_tokens\":11}}"
done
"#;

    fn setup(web_search: bool) -> AskSetup {
        AskSetup {
            system: "You answer questions. Brennholt is spelled like this.".into(),
            model: "sonnet".into(),
            effort: Some(RefineEffort::Low),
            web_search,
        }
    }

    fn question() -> AskInput {
        AskInput::text("<question>\nwhat is the capital of norway\n</question>")
    }

    /// Send one turn and collect the events.
    fn send(
        cli: &mut ClaudeCliAsk,
        setup: &AskSetup,
        input: &AskInput,
        timeout: Duration,
    ) -> (Result<AskReply, AskError>, Vec<AskEvent>) {
        let mut events = Vec::new();
        let result = cli.send(setup, input, &mut |e| events.push(e), &CancelFlag::new(), timeout);
        (result, events)
    }

    const LONG: Duration = Duration::from_secs(10);

    fn wait_for(path: &Path) -> String {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !path.exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        // The file exists before its contents are written.
        thread::sleep(Duration::from_millis(50));
        fs::read_to_string(path).unwrap().trim().to_string()
    }

    fn alive(pid: &str) -> bool {
        // `kill -0` fails once the process is gone.
        Command::new("kill").args(["-0", pid]).stderr(Stdio::null()).status().unwrap().success()
    }

    #[test]
    fn flags_are_refines_lean_ones_plus_streaming_and_a_pinned_permission_mode() {
        let args = spawn_args(&setup(false), Path::new("/tmp/prompt.txt"));
        let joined = args.join(" ");
        for flag in [
            "-p",
            "--safe-mode",
            "--strict-mcp-config",
            "--disable-slash-commands",
            "--no-session-persistence",
            "--input-format stream-json",
            "--output-format stream-json",
            "--verbose",
            "--model sonnet",
            "--effort low",
            "--include-partial-messages",
            "--permission-mode default",
            "--system-prompt-file /tmp/prompt.txt",
        ] {
            assert!(joined.contains(flag), "missing {flag} in {joined}");
        }
        // No tools at all: an empty tool list, nothing allowed.
        let tools = args.iter().position(|a| a == "--tools").unwrap();
        assert_eq!(args[tools + 1], "");
        assert!(!args.iter().any(|a| a == "--allowedTools"));
        assert!(!joined.contains("WebSearch"));
        assert!(!args.iter().any(|a| a == "--bare" || a == "--dangerously-skip-permissions"));
        // Exactly Refine's flags and the three added ones.
        let refine = refine_cli::spawn_args("sonnet", RefineEffort::Low, Path::new("/tmp/prompt.txt"));
        assert_eq!(args[..refine.len()], refine[..]);
        assert_eq!(args[refine.len()..], ["--include-partial-messages", "--permission-mode", "default"]);
    }

    #[test]
    fn web_search_adds_exactly_the_two_websearch_flags() {
        let plain = spawn_args(&setup(false), Path::new("/tmp/p.txt"));
        let web = spawn_args(&setup(true), Path::new("/tmp/p.txt"));
        let tools = web.iter().position(|a| a == "--tools").unwrap();
        assert_eq!(web[tools..tools + 4], ["--tools", "WebSearch", "--allowedTools", "WebSearch"]);
        // Each list is followed by a flag, so nothing after it can be
        // read as one more tool.
        assert!(web[tools + 4].starts_with("--"), "{web:?}");
        // Nothing else differs.
        let mut without = web.clone();
        without.splice(tools + 1..tools + 4, [String::new()]);
        assert_eq!(without, plain);
        assert_eq!(web.iter().filter(|a| *a == "WebSearch").count(), 2);
    }

    #[test]
    fn the_cli_never_gets_an_effort_it_does_not_have() {
        let mut s = setup(false);
        s.effort = Some(RefineEffort::Minimal);
        assert!(spawn_args(&s, Path::new("/tmp/p.txt")).join(" ").contains("--effort low"));
        s.effort = None;
        assert!(spawn_args(&s, Path::new("/tmp/p.txt")).join(" ").contains("--effort low"));
        s.effort = Some(RefineEffort::High);
        assert!(spawn_args(&s, Path::new("/tmp/p.txt")).join(" ").contains("--effort high"));
    }

    #[test]
    fn streams_the_text_in_order_and_reports_first_text_and_usage() {
        let dir = tempfile::tempdir().unwrap();
        let mut cli = ClaudeCliAsk::new(fake_cli(dir.path(), STREAM_OK));
        let setup = setup(false);
        let (reply, events) = send(&mut cli, &setup, &question(), LONG);
        let reply = reply.unwrap();
        // Two text pieces, in order; the thinking never shows.
        assert_eq!(
            events,
            vec![
                AskEvent::Text("The capital of Norway ".into()),
                AskEvent::Text("is Oslo (turn 1).".into()),
            ]
        );
        assert_eq!(reply.text, "The capital of Norway is Oslo (turn 1).");
        let first = reply.first_text_ms.expect("first text time");
        assert!(first <= reply.elapsed_ms, "{first} > {}", reply.elapsed_ms);
        assert_eq!(reply.warm_lead_ms, None, "this call ran cold");
        let usage = reply.usage.unwrap();
        assert_eq!((usage.tokens_in, usage.tokens_cached, usage.tokens_out), (602, 40, 11));
        assert_eq!(usage.cost_usd, Some(0.0025));
        let plan = usage.plan.unwrap();
        assert_eq!(plan.five_hour_used, Some(0.09));
        assert_eq!(plan.seven_day_resets_at, Some(1794646800));

        // The question went in as one user line, as given.
        let sent: Value = serde_json::from_str(&fs::read_to_string(dir.path().join("stdin-1.txt")).unwrap()).unwrap();
        assert_eq!(sent["type"], "user");
        assert_eq!(sent["message"]["content"][0]["text"], "<question>\nwhat is the capital of norway\n</question>");
        // The system prompt went in through the prompt file, and the
        // process was started with the permission mode pinned.
        assert_eq!(fs::read_to_string(dir.path().join("system.txt")).unwrap(), setup.system);
        let args = fs::read_to_string(dir.path().join("args.txt")).unwrap();
        assert!(args.contains("--permission-mode default"), "{args}");
        assert!(args.contains("--include-partial-messages"), "{args}");
        assert!(!args.contains("Brennholt"), "the prompt must not ride on the command line: {args}");
        assert!(!args.contains("WebSearch"), "{args}");
    }

    #[test]
    fn web_search_reaches_the_command_line_as_the_two_flags() {
        let dir = tempfile::tempdir().unwrap();
        let mut cli = ClaudeCliAsk::new(fake_cli(dir.path(), STREAM_OK));
        send(&mut cli, &setup(true), &question(), LONG).0.unwrap();
        let args = fs::read_to_string(dir.path().join("args.txt")).unwrap();
        assert!(args.contains("--tools WebSearch --allowedTools WebSearch --strict-mcp-config"), "{args}");
        assert!(args.contains("--permission-mode default"), "{args}");
    }

    #[test]
    fn a_warmed_process_answers_and_reports_its_head_start() {
        let dir = tempfile::tempdir().unwrap();
        let mut cli = ClaudeCliAsk::new(fake_cli(dir.path(), STREAM_OK));
        let setup = setup(false);
        cli.warm(&setup);
        assert!(cli.process.is_some());
        // Up and waiting on stdin while the person talks.
        thread::sleep(Duration::from_millis(150));
        assert!(!dir.path().join("stdin-1.txt").exists());
        let reply = send(&mut cli, &setup, &question(), LONG).0.unwrap();
        assert_eq!(reply.text, "The capital of Norway is Oslo (turn 1).");
        assert!(reply.warm_lead_ms.unwrap() >= 150);
    }

    #[test]
    fn a_second_turn_goes_to_the_same_process() {
        let dir = tempfile::tempdir().unwrap();
        let mut cli = ClaudeCliAsk::new(fake_cli(dir.path(), STREAM_OK));
        let setup = setup(false);
        let first = send(&mut cli, &setup, &question(), LONG).0.unwrap();
        assert_eq!(first.text, "The capital of Norway is Oslo (turn 1).");
        let (second, events) = send(&mut cli, &setup, &AskInput::text("and of sweden"), LONG);
        // The fake counts turns per process: turn 2 means the same
        // process took the second line.
        let second = second.unwrap();
        assert_eq!(second.text, "The capital of Norway is Oslo (turn 2).");
        assert_eq!(events.len(), 2);
        assert_eq!(second.warm_lead_ms, None, "only a first turn can have had a head start");
        let sent: Value = serde_json::from_str(&fs::read_to_string(dir.path().join("stdin-2.txt")).unwrap()).unwrap();
        assert_eq!(sent["message"]["content"][0]["text"], "and of sweden");
    }

    #[test]
    fn a_changed_setup_starts_a_new_process() {
        let dir = tempfile::tempdir().unwrap();
        let mut cli = ClaudeCliAsk::new(fake_cli(dir.path(), STREAM_OK));
        cli.warm(&setup(false));
        let mut changed = setup(false);
        changed.system = "New instructions.".into();
        let reply = send(&mut cli, &changed, &question(), LONG).0.unwrap();
        assert_eq!(reply.text, "The capital of Norway is Oslo (turn 1).");
        assert_eq!(reply.warm_lead_ms, None);
        assert_eq!(fs::read_to_string(dir.path().join("system.txt")).unwrap(), "New instructions.");
    }

    #[test]
    fn an_image_travels_as_a_base64_block_before_the_text() {
        let input = AskInput {
            text: "what is this".into(),
            images: vec![AskImage { media_type: "image/png".into(), data: vec![0x89, b'P', b'N', b'G'] }],
        };
        let line: Value = serde_json::from_str(&user_line(&input)).unwrap();
        let content = line["message"]["content"].as_array().unwrap();
        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["type"], "image");
        assert_eq!(content[0]["source"]["type"], "base64");
        assert_eq!(content[0]["source"]["media_type"], "image/png");
        assert_eq!(content[0]["source"]["data"], "iVBORw==");
        assert_eq!(content[1], json!({ "type": "text", "text": "what is this" }));
        assert_eq!(line["message"]["role"], "user");
        // One line: the CLI reads its input line by line.
        assert!(!user_line(&input).contains('\n'));
        // And the bytes never show in a debug print.
        assert_eq!(format!("{:?}", input.images[0]), "AskImage(image/png, 4 bytes)");
    }

    /// A turn with a web search in it: the tool call, its result, then
    /// the answer in a second message.
    const SEARCHES: &str = r#"
read -r line
echo '{"type":"stream_event","event":{"type":"message_start"}}'
echo '{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_1","name":"WebSearch","input":{}}}}'
echo '{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"query\": \"ferry"}}}'
echo '{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_1","name":"WebSearch","input":{"query":"ferry timetable Brennholt","mode":"standard"}}]}}'
echo '{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"Web search results for query"}]}}'
echo '{"type":"stream_event","event":{"type":"message_start"}}'
echo '{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"The first ferry leaves at 06:40."}}}'
echo '{"type":"result","subtype":"success","is_error":false,"result":"The first ferry leaves at 06:40."}'
"#;

    #[test]
    fn a_web_search_is_reported_with_its_query_and_its_end() {
        let dir = tempfile::tempdir().unwrap();
        let mut cli = ClaudeCliAsk::new(fake_cli(dir.path(), SEARCHES));
        let (reply, events) = send(&mut cli, &setup(true), &question(), LONG);
        assert_eq!(
            events,
            vec![
                AskEvent::Searching { query: "ferry timetable Brennholt".into() },
                AskEvent::SearchDone,
                AskEvent::Text("The first ferry leaves at 06:40.".into()),
            ]
        );
        assert_eq!(reply.unwrap().text, "The first ferry leaves at 06:40.");
    }

    #[test]
    fn text_before_and_after_a_search_is_two_paragraphs() {
        let dir = tempfile::tempdir().unwrap();
        let body = SEARCHES.replace(
            r#"echo '{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"tool_use""#,
            r#"echo '{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Let me check."}}}'
echo '{"type":"stream_event","event":{"type":"content_block_start","index":1,"content_block":{"type":"tool_use""#,
        );
        let mut cli = ClaudeCliAsk::new(fake_cli(dir.path(), &body));
        let (reply, _) = send(&mut cli, &setup(true), &question(), LONG);
        assert_eq!(reply.unwrap().text, "Let me check.\n\nThe first ferry leaves at 06:40.");
    }

    #[test]
    fn any_tool_but_the_web_search_fails_the_ask() {
        // With web search off, the web search is such a tool too.
        let dir = tempfile::tempdir().unwrap();
        let mut cli = ClaudeCliAsk::new(fake_cli(dir.path(), SEARCHES));
        let (reply, events) = send(&mut cli, &setup(false), &question(), LONG);
        match reply.unwrap_err() {
            AskError::Backend(m) => assert!(m.contains("WebSearch") && m.contains("may not use tools"), "{m}"),
            other => panic!("expected Backend, got {other:?}"),
        }
        assert!(events.is_empty(), "{events:?}");
        // The conversation is over: the process is not kept.
        assert!(cli.process.is_none());

        // With web search on, any other tool still fails.
        let dir = tempfile::tempdir().unwrap();
        let body = SEARCHES.replace("WebSearch", "Bash");
        let mut cli = ClaudeCliAsk::new(fake_cli(dir.path(), &body));
        let (reply, _) = send(&mut cli, &setup(true), &question(), LONG);
        match reply.unwrap_err() {
            AskError::Backend(m) => assert!(m.contains("Bash"), "{m}"),
            other => panic!("expected Backend, got {other:?}"),
        }
    }

    #[test]
    fn a_tool_call_is_caught_on_either_line_that_shows_it() {
        let start = r#"{"type":"stream_event","event":{"type":"content_block_start","content_block":{"type":"tool_use","name":"Write","input":{}}}}"#;
        assert!(matches!(read_line(start, true), Err(AskError::Backend(_))));
        let done = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"ok"},{"type":"tool_use","name":"Bash","input":{"command":"ls"}}]}}"#;
        assert!(matches!(read_line(done, true), Err(AskError::Backend(_))));
        let server = r#"{"type":"assistant","message":{"content":[{"type":"server_tool_use","name":"web_fetch","input":{}}]}}"#;
        assert!(matches!(read_line(server, true), Err(AskError::Backend(_))));
        // Text and thinking blocks are not tools.
        let text = r#"{"type":"stream_event","event":{"type":"content_block_start","content_block":{"type":"text","text":""}}}"#;
        assert_eq!(read_line(text, false), Ok(Line::Other));
        let thinking = r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"thinking_delta","thinking":"hm"}}}"#;
        assert_eq!(read_line(thinking, false), Ok(Line::Other));
        assert_eq!(read_line("not json", false), Ok(Line::Other));
    }

    #[test]
    fn a_cli_that_does_not_stream_still_answers() {
        let dir = tempfile::tempdir().unwrap();
        let settings = fake_cli(
            dir.path(),
            r#"read -r line; echo '{"type":"result","subtype":"success","is_error":false,"result":" Oslo. "}'"#,
        );
        let (reply, events) = send(&mut ClaudeCliAsk::new(settings), &setup(false), &question(), LONG);
        let reply = reply.unwrap();
        assert_eq!(reply.text, "Oslo.");
        assert_eq!(events, vec![AskEvent::Text(" Oslo. ".into())]);
        assert_eq!(reply.usage, None);
    }

    #[test]
    fn cancel_kills_the_process_while_it_is_silent() {
        let dir = tempfile::tempdir().unwrap();
        // Takes the question, then says nothing for a long time.
        let settings = fake_cli(dir.path(), r#"echo $$ > "$(dirname "$0")/pid.txt"; read -r line; sleep 30"#);
        let mut cli = ClaudeCliAsk::new(settings);
        // Up and running before the question goes in (the first run
        // of a new script can take a while to start on macOS).
        cli.warm(&setup(false));
        let pid = wait_for(&dir.path().join("pid.txt"));
        assert!(alive(&pid));
        let cancel = CancelFlag::new();
        let canceller = {
            let cancel = cancel.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(300));
                cancel.cancel();
            })
        };
        let started = Instant::now();
        let err = cli.send(&setup(false), &question(), &mut |_| {}, &cancel, Duration::from_secs(20)).unwrap_err();
        canceller.join().unwrap();
        assert_eq!(err, AskError::Cancelled);
        assert!(started.elapsed() < Duration::from_secs(3), "{:?}", started.elapsed());
        assert!(!alive(&pid), "process {pid} survived the cancel");
        assert!(cli.process.is_none());
    }

    /// A question far larger than a pipe holds, for a CLI that never
    /// reads it.
    fn huge_question() -> AskInput {
        AskInput {
            text: "what is on this screen".into(),
            images: vec![AskImage { media_type: "image/png".into(), data: vec![7u8; 2 * 1024 * 1024] }],
        }
    }

    /// Never reads its input, and stays for half a minute.
    const DEAF: &str = r#"echo $$ > "$(dirname "$0")/pid.txt"; sleep 30"#;

    #[test]
    fn a_question_the_cli_does_not_read_can_still_be_cancelled() {
        let dir = tempfile::tempdir().unwrap();
        let mut cli = ClaudeCliAsk::new(fake_cli(dir.path(), DEAF));
        cli.warm(&setup(false));
        let pid = wait_for(&dir.path().join("pid.txt"));
        let cancel = CancelFlag::new();
        let canceller = {
            let cancel = cancel.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(300));
                cancel.cancel();
            })
        };
        let started = Instant::now();
        let err = cli.send(&setup(false), &huge_question(), &mut |_| {}, &cancel, Duration::from_secs(20)).unwrap_err();
        canceller.join().unwrap();
        assert_eq!(err, AskError::Cancelled);
        assert!(started.elapsed() < Duration::from_secs(3), "{:?}", started.elapsed());
        assert!(!alive(&pid), "process {pid} survived the cancel");
    }

    #[test]
    fn a_question_the_cli_does_not_read_still_times_out() {
        let dir = tempfile::tempdir().unwrap();
        let mut cli = ClaudeCliAsk::new(fake_cli(dir.path(), DEAF));
        cli.warm(&setup(false));
        let pid = wait_for(&dir.path().join("pid.txt"));
        let started = Instant::now();
        let (reply, _) = send(&mut cli, &setup(false), &huge_question(), Duration::from_millis(300));
        assert_eq!(reply.unwrap_err(), AskError::Timeout);
        assert!(started.elapsed() < Duration::from_secs(3), "{:?}", started.elapsed());
        assert!(!alive(&pid), "process {pid} survived the timeout");
    }

    #[test]
    fn a_large_question_reaches_a_cli_that_reads_it() {
        let dir = tempfile::tempdir().unwrap();
        let mut cli = ClaudeCliAsk::new(fake_cli(dir.path(), STREAM_OK));
        let reply = send(&mut cli, &setup(false), &huge_question(), LONG).0.unwrap();
        assert_eq!(reply.text, "The capital of Norway is Oslo (turn 1).");
        // All of it arrived, and stdin is back for a further turn.
        let sent = fs::read_to_string(dir.path().join("stdin-1.txt")).unwrap();
        assert!(sent.len() > 2 * 1024 * 1024, "{}", sent.len());
        let again = send(&mut cli, &setup(false), &AskInput::text("and of sweden"), LONG).0.unwrap();
        assert_eq!(again.text, "The capital of Norway is Oslo (turn 2).");
    }

    #[test]
    fn a_slow_cli_times_out_and_is_killed() {
        let dir = tempfile::tempdir().unwrap();
        let settings = fake_cli(dir.path(), r#"echo $$ > "$(dirname "$0")/pid.txt"; read -r line; sleep 30"#);
        let mut cli = ClaudeCliAsk::new(settings);
        cli.warm(&setup(false));
        let pid = wait_for(&dir.path().join("pid.txt"));
        assert!(alive(&pid));
        let started = Instant::now();
        let (reply, _) = send(&mut cli, &setup(false), &question(), Duration::from_millis(300));
        assert_eq!(reply.unwrap_err(), AskError::Timeout);
        assert!(started.elapsed() < Duration::from_secs(3), "{:?}", started.elapsed());
        assert!(!alive(&pid), "process {pid} survived the timeout");
        assert!(cli.process.is_none());
    }

    #[test]
    fn dropping_the_backend_ends_the_conversation() {
        let dir = tempfile::tempdir().unwrap();
        let body = format!("echo $$ > \"$(dirname \"$0\")/pid.txt\"\n{STREAM_OK}");
        let mut cli = ClaudeCliAsk::new(fake_cli(dir.path(), &body));
        send(&mut cli, &setup(false), &question(), LONG).0.unwrap();
        let pid = wait_for(&dir.path().join("pid.txt"));
        // Still there, waiting for a further turn.
        assert!(alive(&pid));
        let args = fs::read_to_string(dir.path().join("args.txt")).unwrap();
        let prompt_file = args.split_whitespace().skip_while(|a| *a != "--system-prompt-file").nth(1).unwrap().to_string();
        assert!(Path::new(&prompt_file).exists());
        drop(cli);
        assert!(!alive(&pid), "process {pid} survived the drop");
        assert!(!Path::new(&prompt_file).exists(), "{prompt_file} was left behind");
    }

    #[test]
    fn errors_read_like_refines() {
        // An error result.
        let dir = tempfile::tempdir().unwrap();
        let settings = fake_cli(
            dir.path(),
            r#"read -r line; echo '{"type":"result","subtype":"success","is_error":true,"result":"API Error: 529 overloaded"}'"#,
        );
        let (reply, _) = send(&mut ClaudeCliAsk::new(settings), &setup(false), &question(), LONG);
        assert_eq!(reply.unwrap_err(), AskError::Backend("Claude Code: API Error: 529 overloaded".into()));

        // Signed out.
        let dir = tempfile::tempdir().unwrap();
        let settings = fake_cli(
            dir.path(),
            r#"read -r line; echo '{"type":"result","subtype":"success","is_error":true,"result":"Invalid API key · Please run /login"}'"#,
        );
        let (reply, _) = send(&mut ClaudeCliAsk::new(settings), &setup(false), &question(), LONG);
        assert_eq!(reply.unwrap_err(), AskError::Unavailable(refine_cli::NOT_SIGNED_IN.into()));

        // Exit without a result: what it wrote to stderr.
        let dir = tempfile::tempdir().unwrap();
        let settings = fake_cli(dir.path(), "echo \"error: unknown option '--permission-mode'\" >&2; exit 1");
        let (reply, _) = send(&mut ClaudeCliAsk::new(settings), &setup(false), &question(), LONG);
        assert_eq!(
            reply.unwrap_err(),
            AskError::Backend("Claude Code: error: unknown option '--permission-mode'".into())
        );

        // Not installed.
        let settings = ClaudeCliSettings { path: Some("/nonexistent/claude".into()), ..ClaudeCliSettings::default() };
        let (reply, _) = send(&mut ClaudeCliAsk::new(settings), &setup(false), &question(), LONG);
        assert_eq!(reply.unwrap_err(), AskError::Unavailable(refine_cli::NOT_INSTALLED.into()));
    }
}

/// The same checks against a fake `claude.cmd`, for the Windows box.
///
/// Written on macOS and compile-checked for `x86_64-pc-windows-gnu`,
/// but not yet run: the first `cargo test` on Windows is what verifies
/// streaming through a `.cmd` shim, the cancel and the kill, including
/// that the process behind the shim is gone after a cancel and after
/// a timeout ([`refine_cli::end_process`]).
#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;

    fn fake_cli(dir: &Path, body: &str) -> ClaudeCliSettings {
        let path = dir.join("claude.cmd");
        fs::write(&path, format!("@echo off\r\n{body}\r\n")).unwrap();
        ClaudeCliSettings { path: Some(path.display().to_string()), ..ClaudeCliSettings::default() }
    }

    const ANSWER_OK: &str = "set /p line=\r\n\
echo {\"type\":\"stream_event\",\"event\":{\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"Oslo \"}}}\r\n\
echo {\"type\":\"stream_event\",\"event\":{\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"it is.\"}}}\r\n\
echo {\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"result\":\"Oslo it is.\"}\r\n\
exit /b 0";

    fn setup() -> AskSetup {
        AskSetup { system: "You answer questions.".into(), model: "sonnet".into(), effort: None, web_search: false }
    }

    #[test]
    fn streams_through_a_cmd_shim() {
        let dir = tempfile::tempdir().unwrap();
        let mut events = Vec::new();
        let reply = ClaudeCliAsk::new(fake_cli(dir.path(), ANSWER_OK))
            .send(
                &setup(),
                &AskInput::text("what is the capital of norway"),
                &mut |e| events.push(e),
                &CancelFlag::new(),
                Duration::from_secs(10),
            )
            .unwrap();
        assert_eq!(reply.text, "Oslo it is.");
        assert_eq!(events, vec![AskEvent::Text("Oslo ".into()), AskEvent::Text("it is.".into())]);
    }

    #[test]
    fn warm_then_ask_through_a_cmd_shim() {
        let dir = tempfile::tempdir().unwrap();
        let mut cli = ClaudeCliAsk::new(fake_cli(dir.path(), ANSWER_OK));
        cli.warm(&setup());
        assert!(cli.process.is_some());
        thread::sleep(Duration::from_millis(300));
        let reply = cli
            .send(&setup(), &AskInput::text("hello"), &mut |_| {}, &CancelFlag::new(), Duration::from_secs(10))
            .unwrap();
        assert_eq!(reply.text, "Oslo it is.");
        assert!(reply.warm_lead_ms.is_some());
    }

    #[test]
    fn a_slow_cmd_shim_times_out() {
        let dir = tempfile::tempdir().unwrap();
        let settings = fake_cli(dir.path(), "set /p line=\r\nping -n 30 127.0.0.1 >nul");
        let started = Instant::now();
        let err = ClaudeCliAsk::new(settings)
            .send(&setup(), &AskInput::text("hello"), &mut |_| {}, &CancelFlag::new(), Duration::from_millis(500))
            .unwrap_err();
        assert_eq!(err, AskError::Timeout);
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    /// A shim that takes the question and then starts a process of
    /// its own that runs for half a minute: the place the real CLI
    /// has behind `claude.cmd`. That process writes its id to
    /// `pid.txt` next to the shim.
    const SLOW_GRANDCHILD: &str = "set /p line=\r\n\
powershell -NoProfile -Command \"$PID | Out-File -Encoding ascii '%~dp0pid.txt'; Start-Sleep -Seconds 30\"";

    /// The id the grandchild wrote, once it is up.
    fn grandchild_pid(dir: &Path) -> String {
        let file = dir.join("pid.txt");
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let pid = fs::read_to_string(&file).unwrap_or_default().trim().to_string();
            if !pid.is_empty() {
                return pid;
            }
            assert!(Instant::now() < deadline, "the grandchild never started");
            thread::sleep(Duration::from_millis(50));
        }
    }

    /// `tasklist` still knows the process `pid`.
    fn alive(pid: &str) -> bool {
        let out = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).contains(&format!("\"{pid}\""))
    }

    #[test]
    fn cancel_ends_the_process_behind_the_cmd_shim() {
        let dir = tempfile::tempdir().unwrap();
        let cancel = CancelFlag::new();
        // Cancel once the grandchild is up and has said who it is.
        let canceller = {
            let cancel = cancel.clone();
            let dir = dir.path().to_path_buf();
            thread::spawn(move || {
                let pid = grandchild_pid(&dir);
                assert!(alive(&pid), "the grandchild {pid} is not running");
                cancel.cancel();
                pid
            })
        };
        let err = ClaudeCliAsk::new(fake_cli(dir.path(), SLOW_GRANDCHILD))
            .send(&setup(), &AskInput::text("hello"), &mut |_| {}, &cancel, Duration::from_secs(25))
            .unwrap_err();
        let pid = canceller.join().unwrap();
        assert_eq!(err, AskError::Cancelled);
        // The process is dropped when `send` returns an error, and
        // the tree with it: `taskkill` has finished by then.
        assert!(!alive(&pid), "the process behind the shim ({pid}) outlived the cancel");
    }

    #[test]
    fn a_timeout_ends_the_process_behind_the_cmd_shim() {
        let dir = tempfile::tempdir().unwrap();
        let pid = {
            let dir = dir.path().to_path_buf();
            thread::spawn(move || grandchild_pid(&dir))
        };
        let err = ClaudeCliAsk::new(fake_cli(dir.path(), SLOW_GRANDCHILD))
            .send(&setup(), &AskInput::text("hello"), &mut |_| {}, &CancelFlag::new(), Duration::from_secs(8))
            .unwrap_err();
        assert_eq!(err, AskError::Timeout);
        let pid = pid.join().unwrap();
        assert!(!alive(&pid), "the process behind the shim ({pid}) outlived the timeout");
    }

    #[test]
    fn cancel_stops_a_cmd_shim() {
        let dir = tempfile::tempdir().unwrap();
        let settings = fake_cli(dir.path(), "set /p line=\r\nping -n 30 127.0.0.1 >nul");
        let cancel = CancelFlag::new();
        let canceller = {
            let cancel = cancel.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(300));
                cancel.cancel();
            })
        };
        let started = Instant::now();
        let err = ClaudeCliAsk::new(settings)
            .send(&setup(), &AskInput::text("hello"), &mut |_| {}, &cancel, Duration::from_secs(20))
            .unwrap_err();
        canceller.join().unwrap();
        assert_eq!(err, AskError::Cancelled);
        assert!(started.elapsed() < Duration::from_secs(10));
    }
}
