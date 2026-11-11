//! The agent backend against fake `claude` scripts: every branch of
//! what the CLI can say about a tool, and what goes back to it.
//!
//! No model and no real connection is involved. Each script prints
//! the lines the real CLI prints (their shapes are from runs against
//! Claude Code 2.1.322, TASK-99 notes), and where the real CLI waits
//! for an answer on stdin, the script reads one and writes it to a
//! file for the test to look at.

use super::*;
use crate::agent::fixture;
use crate::ask::{AskPlan, ConfirmDecision, ContextSource, answer_confirm};
use crate::settings::{
    AgentConnection, AgentConnectionKind, AgentSettings, AskSettings, CustomServer, RefineSettings,
};
use serde_json::json;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Instant;

const SECRET: &str = "tok-made-up-321";

/// A fake `claude`: a shell script standing in for the real CLI.
fn fake_cli(dir: &Path, body: &str) -> ClaudeCliSettings {
    let path = dir.join("claude");
    fs::write(&path, format!("#!/bin/sh\n{PRELUDE}\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    ClaudeCliSettings { path: Some(path.display().to_string()), ..ClaudeCliSettings::default() }
}

/// Every script starts the same way: record the arguments and the
/// two files it was handed, read the question, say that it is up.
const PRELUDE: &str = r#"
here="$(dirname "$0")"
echo "$@" > "$here/args.txt"
echo $$ > "$here/pid.txt"
prev=""; for a in "$@"; do
  if [ "$prev" = "--system-prompt-file" ]; then cp "$a" "$here/system.txt"; fi
  if [ "$prev" = "--mcp-config" ]; then cp "$a" "$here/mcp.json"; echo "$a" > "$here/mcp-path.txt"; fi
  prev="$a"
done
read -r question
printf '%s\n' "$question" > "$here/question.txt"
echo '{"type":"system","subtype":"init","mcp_servers":[{"name":"chat","status":"connected"}]}'
say() {
  echo '{"type":"stream_event","event":{"type":"message_start"}}'
  echo "{\"type\":\"stream_event\",\"event\":{\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"$1\"}}}"
  echo "{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"$1\"}]}}"
}
done_with() {
  echo "{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"result\":\"$1\",\"total_cost_usd\":0.004,\"usage\":{\"input_tokens\":3,\"cache_creation_input_tokens\":700,\"cache_read_input_tokens\":900,\"output_tokens\":40}}"
}
"#;

const SEND_INPUT: &str = r#"{"to":"bo@example.com","body":"Room confirmed"}"#;

/// The model calls `send_message`, the CLI asks, and what happens
/// next depends on the answer. `ID` is the request id of the test.
const ASKS_TO_SEND: &str = r#"
echo '{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_send","name":"mcp__chat__send_message","input":{}}}}'
echo '{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_send","name":"mcp__chat__send_message","input":{"to":"bo@example.com","body":"Room confirmed"}}]},"tool_use_meta":[{"id":"toolu_send","display_name":"Send Message","tool_name":"send_message"}]}'
echo '{"type":"control_request","request_id":"ID","request":{"subtype":"can_use_tool","tool_name":"mcp__chat__send_message","display_name":"Send Message","input":{"to":"bo@example.com","body":"Room confirmed"},"tool_use_id":"toolu_send"}}'
read -r answer
printf '%s\n' "$answer" > "$here/answer.txt"
case "$answer" in
  *'"behavior":"allow"'*)
    echo '{"type":"user","message":{"role":"user","content":[{"tool_use_id":"toolu_send","type":"tool_result","content":[{"type":"text","text":"ok: message sent (id m-2)"}]}]}}'
    say "I sent Bo the message."
    done_with "I sent Bo the message."
    ;;
  *)
    echo '{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"declined","is_error":true,"tool_use_id":"toolu_send"}]},"tool_result_meta":[{"id":"toolu_send","non_execution_kind":"permission-rule"}]}'
    AFTER
    say "The message was not sent."
    done_with "The message was not sent."
    ;;
esac
"#;

fn asks_to_send(request_id: &str, after_a_no: &str) -> String {
    ASKS_TO_SEND.replace("ID", request_id).replace("AFTER", after_a_no)
}

fn chat(kind: AgentConnectionKind) -> AgentConnection {
    AgentConnection {
        name: "chat".into(),
        kind,
        enabled: true,
        allowed_tools: vec![fixture::READ_THREAD.into(), fixture::CREATE_DRAFT.into(), fixture::SEND_MESSAGE.into()],
        tools: fixture::tools(),
        custom: (kind == AgentConnectionKind::Custom).then(|| CustomServer {
            command: "/opt/example/chat-server".into(),
            args: vec!["--stdio".into()],
            env: std::collections::BTreeMap::from([("CHAT_TOKEN".to_string(), SECRET.to_string())]),
            ..CustomServer::default()
        }),
    }
}

/// Reads and drafts run, a send asks, search and delete are not
/// ticked.
fn run() -> AgentRun {
    let settings = AgentSettings {
        enabled: true,
        drafts_only: false,
        connections: vec![
            chat(AgentConnectionKind::Custom),
            // Switched off: it must not reach the run.
            AgentConnection { name: "other".into(), enabled: false, ..chat(AgentConnectionKind::Custom) },
        ],
        ..AgentSettings::default()
    };
    AgentRun::new(&settings, None).expect("a run with tools")
}

fn setup(web_search: bool) -> AskSetup {
    AskSetup {
        system: "You answer questions. Your tools: the chat.".into(),
        model: "sonnet".into(),
        effort: Some(RefineEffort::Low),
        web_search,
    }
}

fn question() -> AskInput {
    AskInput::text("<question>\nsend bo a message that the room is confirmed\n</question>")
}

const LONG: Duration = Duration::from_secs(10);

/// Send one turn. `on_confirm` is called from inside the sink for
/// every confirm, with the cancel flag of the turn.
fn send(
    agent: &mut ClaudeAgent,
    timeout: Duration,
    mut on_confirm: impl FnMut(&AskConfirm, &CancelFlag),
) -> (Result<AskReply, AskError>, Vec<AskEvent>) {
    let mut events = Vec::new();
    let cancel = CancelFlag::new();
    let result = agent.send(
        &setup(false),
        &question(),
        &mut |event| {
            if let AskEvent::Confirm(confirm) = &event {
                on_confirm(confirm, &cancel);
            }
            events.push(event);
        },
        &cancel,
        timeout,
    );
    (result, events)
}

fn steps_of(events: &[AskEvent]) -> Vec<(String, StepState, String)> {
    events
        .iter()
        .filter_map(|event| match event {
            AskEvent::Step(step) => Some((step.tool.clone(), step.state, step.outcome.clone())),
            _ => None,
        })
        .collect()
}

fn text_of(events: &[AskEvent]) -> String {
    events.iter().filter_map(|event| if let AskEvent::Text(piece) = event { Some(piece.as_str()) } else { None }).collect()
}

fn confirms_of(events: &[AskEvent]) -> Vec<&AskConfirm> {
    events.iter().filter_map(|event| if let AskEvent::Confirm(confirm) = event { Some(confirm) } else { None }).collect()
}

fn closed_of(events: &[AskEvent]) -> Vec<(String, ConfirmOutcome)> {
    events
        .iter()
        .filter_map(|event| match event {
            AskEvent::ConfirmClosed { request_id, outcome } => Some((request_id.clone(), *outcome)),
            _ => None,
        })
        .collect()
}

fn answer_file(dir: &Path, name: &str) -> Value {
    serde_json::from_str(&fs::read_to_string(dir.join(name)).unwrap()).unwrap()
}

fn alive(pid: &str) -> bool {
    // `kill -0` fails once the process is gone.
    Command::new("kill").args(["-0", pid]).stderr(Stdio::null()).status().unwrap().success()
}

// --- the arguments -------------------------------------------------

#[test]
fn the_arguments_of_an_agent_ask_are_pinned() {
    let args = spawn_args(&setup(false), &run(), Path::new("/tmp/run.json"), Path::new("/tmp/prompt.txt"));
    assert_eq!(
        args,
        [
            "-p",
            "--model",
            "sonnet",
            "--effort",
            "low",
            "--setting-sources",
            "",
            "--tools",
            "",
            "--strict-mcp-config",
            "--mcp-config",
            "/tmp/run.json",
            "--allowedTools",
            "mcp__chat__read_thread,mcp__chat__create_draft",
            "--disallowedTools",
            "mcp__chat__search_messages,mcp__chat__delete_message",
            "--permission-mode",
            "default",
            "--permission-prompt-tool",
            "stdio",
            "--disable-slash-commands",
            "--no-session-persistence",
            "--max-turns",
            "12",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
            "--include-partial-messages",
            "--system-prompt-file",
            "/tmp/prompt.txt",
        ]
    );
    // The tool that asks is on neither list: that is what makes the
    // CLI ask.
    assert!(!args.iter().any(|a| a.contains("send_message")));
    // Nothing that would widen it, and not the flag that would drop
    // the connections.
    for flag in ["--safe-mode", "--bare", "--dangerously-skip-permissions", "--resume", "--continue", "--settings"] {
        assert!(!args.iter().any(|a| a == flag), "{flag} in {args:?}");
    }
    assert!(!args.iter().any(|a| a == "auto" || a == "bypassPermissions" || a == "acceptEdits" || a == "dontAsk"));
}

#[test]
fn every_flag_that_takes_a_list_is_followed_by_a_flag_and_an_empty_list_is_left_out() {
    let follows_a_list = |args: &[String]| {
        for (at, arg) in args.iter().enumerate() {
            if ["--tools", "--mcp-config", "--allowedTools", "--disallowedTools"].contains(&arg.as_str()) {
                let after = &args[at + 2];
                assert!(after.starts_with("--"), "{arg} is followed by a value and then {after}: {args:?}");
            }
        }
    };
    let full = spawn_args(&setup(false), &run(), Path::new("/tmp/run.json"), Path::new("/tmp/p.txt"));
    follows_a_list(&full);
    // Only a send ticked: nothing runs by itself, so no allow list.
    let mut only_acts = run();
    only_acts.tools.allowed.clear();
    let args = spawn_args(&setup(false), &only_acts, Path::new("/tmp/run.json"), Path::new("/tmp/p.txt"));
    assert!(!args.iter().any(|a| a == "--allowedTools"), "{args:?}");
    follows_a_list(&args);
    // Everything ticked: nothing to take away.
    let mut all = run();
    all.tools.disallowed.clear();
    let args = spawn_args(&setup(false), &all, Path::new("/tmp/run.json"), Path::new("/tmp/p.txt"));
    assert!(!args.iter().any(|a| a == "--disallowedTools"), "{args:?}");
    follows_a_list(&args);
    // The question is never an argument.
    assert!(!full.iter().any(|a| a.contains("question") || a.contains("room")));
}

#[test]
fn web_search_stays_the_asks_own_setting() {
    let off = spawn_args(&setup(false), &run(), Path::new("/tmp/run.json"), Path::new("/tmp/p.txt"));
    let on = spawn_args(&setup(true), &run(), Path::new("/tmp/run.json"), Path::new("/tmp/p.txt"));
    let tools = on.iter().position(|a| a == "--tools").unwrap();
    assert_eq!(on[tools + 1], "WebSearch");
    let allowed = on.iter().position(|a| a == "--allowedTools").unwrap();
    assert_eq!(on[allowed + 1], "WebSearch,mcp__chat__read_thread,mcp__chat__create_draft");
    // Nothing else differs.
    let mut back = on.clone();
    back[tools + 1] = String::new();
    back[allowed + 1] = "mcp__chat__read_thread,mcp__chat__create_draft".into();
    assert_eq!(back, off);
    // No effort the CLI does not have.
    let mut minimal = setup(false);
    minimal.effort = Some(RefineEffort::Minimal);
    assert!(spawn_args(&minimal, &run(), Path::new("/a"), Path::new("/b")).join(" ").contains("--effort low"));
}

// --- a call that runs ----------------------------------------------

const READS_THE_THREAD: &str = r#"
echo '{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_read","name":"mcp__chat__read_thread","input":{}}}}'
echo '{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_read","name":"mcp__chat__read_thread","input":{"thread_id":"T1"}}]},"tool_use_meta":[{"id":"toolu_read","display_name":"Read Thread","tool_name":"read_thread"}]}'
printf '%s\n' '{"type":"user","message":{"role":"user","content":[{"tool_use_id":"toolu_read","type":"tool_result","content":[{"type":"text","text":"Thread #release-plan\nAnna: Can we move the release review to Thursday 14:00?"}]}]},"tool_result_meta":[{"id":"toolu_read","permission_decision":{"decision":"accept","source":"config"}}]}'
say "Anna asked to move the review to Thursday at 14:00."
done_with "Anna asked to move the review to Thursday at 14:00."
"#;

#[test]
fn an_allowed_call_and_its_result_are_a_step_that_starts_and_ends() {
    let dir = tempfile::tempdir().unwrap();
    let mut agent = ClaudeAgent::new(fake_cli(dir.path(), READS_THE_THREAD), run());
    let (reply, events) = send(&mut agent, LONG, |_, _| panic!("nothing to confirm"));
    let reply = reply.unwrap();
    assert_eq!(reply.text, "Anna asked to move the review to Thursday at 14:00.");
    assert_eq!(reply.notice, "");
    assert_eq!(reply.usage.as_ref().map(|u| u.tokens_out), Some(40));
    assert_eq!(
        steps_of(&events),
        [
            ("read_thread".to_string(), StepState::Running, String::new()),
            (
                "read_thread".to_string(),
                StepState::Done,
                "Thread #release-plan Anna: Can we move the release review to Thursday 14:00?".to_string()
            ),
        ]
    );
    let AskEvent::Step(step) = &events[0] else { panic!("{events:?}") };
    assert_eq!(
        (step.id.as_str(), step.connection.as_str(), step.title.as_str(), step.kind, step.input_summary.as_str()),
        ("toolu_read", "chat", "Read Thread", ToolKind::Read, "thread_id: T1")
    );
    assert_eq!(text_of(&events), "Anna asked to move the review to Thursday at 14:00.");
    assert!(confirms_of(&events).is_empty());

    // What the process was started with: the pinned flags, the
    // prompt in its file, the question on stdin and nowhere else.
    let args = fs::read_to_string(dir.path().join("args.txt")).unwrap();
    for flag in [
        "--permission-mode default",
        "--strict-mcp-config --mcp-config ",
        "--permission-prompt-tool stdio",
        "--no-session-persistence",
        "--allowedTools mcp__chat__read_thread,mcp__chat__create_draft --disallowedTools mcp__chat__search_messages,mcp__chat__delete_message",
        "--max-turns 12",
    ] {
        assert!(args.contains(flag), "{flag} missing in {args}");
    }
    assert!(!args.contains("--safe-mode") && !args.contains("room is confirmed"), "{args}");
    assert_eq!(fs::read_to_string(dir.path().join("system.txt")).unwrap(), setup(false).system);
    let sent: Value = serde_json::from_str(&fs::read_to_string(dir.path().join("question.txt")).unwrap()).unwrap();
    assert_eq!(sent["message"]["content"][0]["text"], question().text);

    // The connections it was given: the enabled one, as the person
    // entered it, and not the one that is switched off.
    let given = answer_file(dir.path(), "mcp.json");
    let servers = given["mcpServers"].as_object().unwrap();
    assert_eq!(servers.keys().collect::<Vec<_>>(), ["chat"]);
    assert_eq!(servers["chat"]["command"], "/opt/example/chat-server");
    assert_eq!(servers["chat"]["env"]["CHAT_TOKEN"], SECRET);
    // The file is private, and it goes when the conversation does.
    let path = PathBuf::from(fs::read_to_string(dir.path().join("mcp-path.txt")).unwrap().trim());
    assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    drop(agent);
    assert!(!path.exists(), "the connection file outlived the process");
}

#[test]
fn a_tool_that_reports_an_error_is_a_failed_step_and_the_ask_goes_on() {
    let dir = tempfile::tempdir().unwrap();
    let body = r#"
echo '{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_read","name":"mcp__chat__read_thread","input":{}}]}}'
echo '{"type":"user","message":{"role":"user","content":[{"tool_use_id":"toolu_read","type":"tool_result","content":"The team chat did not answer.","is_error":true}]}}'
say "I could not read the thread: the chat did not answer."
done_with "I could not read the thread: the chat did not answer."
"#;
    let mut agent = ClaudeAgent::new(fake_cli(dir.path(), body), run());
    let (reply, events) = send(&mut agent, LONG, |_, _| panic!("nothing to confirm"));
    assert!(reply.unwrap().text.contains("could not read"));
    assert_eq!(
        steps_of(&events)[1],
        ("read_thread".to_string(), StepState::Failed, "The team chat did not answer.".to_string())
    );
    // Without the CLI's nicer name, the title is the fixture's.
    let AskEvent::Step(step) = &events[0] else { panic!("{events:?}") };
    assert_eq!((step.title.as_str(), step.input_summary.as_str()), ("Read Thread", ""));
}

// --- the confirm ---------------------------------------------------

#[test]
fn a_confirm_that_is_allowed_hands_the_input_back_unchanged_and_the_tool_runs() {
    let dir = tempfile::tempdir().unwrap();
    let mut agent = ClaudeAgent::new(fake_cli(dir.path(), &asks_to_send("req-claude-allow", "")), run());
    let mut shown = Vec::new();
    let (reply, events) = send(&mut agent, LONG, |confirm, _| {
        shown.push(confirm.clone());
        assert!(answer_confirm(&confirm.request_id, ConfirmDecision::Allow));
    });
    assert_eq!(reply.unwrap().text, "I sent Bo the message.");

    // The person was shown the tool and the whole input.
    assert_eq!(shown.len(), 1);
    let confirm = &shown[0];
    assert_eq!(
        (confirm.request_id.as_str(), confirm.step_id.as_str(), confirm.connection.as_str(), confirm.tool.as_str()),
        ("req-claude-allow", "toolu_send", "chat", "send_message")
    );
    assert_eq!((confirm.title.as_str(), confirm.timeout_s, confirm.input_cut), ("Send Message", 120, false));
    let expected: Value = serde_json::from_str(SEND_INPUT).unwrap();
    assert_eq!(serde_json::from_str::<Value>(&confirm.input).unwrap(), expected);

    // The CLI got a yes with exactly that input.
    let answer = answer_file(dir.path(), "answer.txt");
    assert_eq!(answer["type"], "control_response");
    assert_eq!(answer["response"]["subtype"], "success");
    assert_eq!(answer["response"]["request_id"], "req-claude-allow");
    assert_eq!(answer["response"]["response"]["behavior"], "allow");
    assert_eq!(answer["response"]["response"]["updatedInput"], expected);

    assert_eq!(
        steps_of(&events),
        [
            ("send_message".to_string(), StepState::Running, String::new()),
            ("send_message".to_string(), StepState::Waiting, String::new()),
            ("send_message".to_string(), StepState::Running, String::new()),
            ("send_message".to_string(), StepState::Done, "ok: message sent (id m-2)".to_string()),
        ]
    );
    assert_eq!(closed_of(&events), [("req-claude-allow".to_string(), ConfirmOutcome::Allowed)]);
    // The confirm came after the step said it waits, and before it
    // was closed.
    let at = |wanted: &dyn Fn(&AskEvent) -> bool| events.iter().position(|e| wanted(e)).unwrap();
    let waits = at(&|e| matches!(e, AskEvent::Step(s) if s.state == StepState::Waiting));
    let asked = at(&|e| matches!(e, AskEvent::Confirm(_)));
    let closed = at(&|e| matches!(e, AskEvent::ConfirmClosed { .. }));
    assert!(waits < asked && asked < closed, "{events:?}");
    let AskEvent::Step(step) = &events[waits] else { unreachable!() };
    assert_eq!(step.kind, ToolKind::Act);
    // A late answer to a confirm that is over finds nothing.
    assert!(!answer_confirm("req-claude-allow", ConfirmDecision::Deny));
}

#[test]
fn a_confirm_that_is_denied_never_runs_the_tool_and_the_model_is_told() {
    let dir = tempfile::tempdir().unwrap();
    let mut agent = ClaudeAgent::new(fake_cli(dir.path(), &asks_to_send("req-claude-deny", "")), run());
    let (reply, events) = send(&mut agent, LONG, |confirm, _| {
        assert!(answer_confirm(&confirm.request_id, ConfirmDecision::Deny));
    });
    assert_eq!(reply.unwrap().text, "The message was not sent.");
    let answer = answer_file(dir.path(), "answer.txt");
    assert_eq!(answer["response"]["response"]["behavior"], "deny");
    assert_eq!(answer["response"]["response"]["message"], claude_cli::DENIED_BY_PERSON);
    assert!(answer["response"]["response"].get("updatedInput").is_none());
    // The step ends as denied, and what the CLI reports about the
    // refused call afterwards does not turn it into a failure.
    let steps = steps_of(&events);
    assert_eq!(steps.last().unwrap(), &("send_message".to_string(), StepState::Denied, "You said no.".to_string()));
    assert!(!steps.iter().any(|(_, state, _)| matches!(state, StepState::Failed | StepState::Done)));
    assert_eq!(closed_of(&events), [("req-claude-deny".to_string(), ConfirmOutcome::Denied)]);
}

#[test]
fn a_confirm_nobody_answers_is_a_no_and_does_not_eat_the_asks_own_time() {
    let dir = tempfile::tempdir().unwrap();
    // After the no, the model takes its time over the answer.
    let mut agent = ClaudeAgent::new(fake_cli(dir.path(), &asks_to_send("req-claude-timeout", "sleep 1.2")), {
        let mut run = run();
        run.confirm_timeout_s = 3;
        run
    });
    let started = Instant::now();
    // The ask's own timeout is shorter than the confirm's wait plus
    // the answer: it still answers, because the wait does not count.
    let (reply, events) = send(&mut agent, Duration::from_secs(4), |confirm, _| {
        assert_eq!(confirm.timeout_s, 3);
    });
    assert_eq!(reply.unwrap().text, "The message was not sent.");
    assert!(started.elapsed() >= Duration::from_millis(4100), "{:?}", started.elapsed());
    let answer = answer_file(dir.path(), "answer.txt");
    assert_eq!(answer["response"]["response"]["behavior"], "deny");
    assert_eq!(answer["response"]["response"]["message"], claude_cli::DENIED_BY_TIMEOUT);
    assert_eq!(closed_of(&events), [("req-claude-timeout".to_string(), ConfirmOutcome::TimedOut)]);
    assert_eq!(
        steps_of(&events).last().unwrap(),
        &("send_message".to_string(), StepState::Denied, "No answer in time, so it was not done.".to_string())
    );
    // An answer that comes after the time is up changes nothing.
    assert!(!answer_confirm("req-claude-timeout", ConfirmDecision::Allow));
}

#[test]
fn a_cancel_while_a_confirm_waits_is_a_no_and_ends_the_process() {
    let dir = tempfile::tempdir().unwrap();
    // A CLI that would go on and send if it were allowed to live.
    let mut agent = ClaudeAgent::new(fake_cli(dir.path(), &asks_to_send("req-claude-cancel", "sleep 30")), run());
    let started = Instant::now();
    let (reply, events) = send(&mut agent, LONG, |_, cancel| cancel.cancel());
    assert_eq!(reply, Err(AskError::Cancelled));
    assert!(started.elapsed() < Duration::from_secs(3), "{:?}", started.elapsed());
    assert_eq!(closed_of(&events), [("req-claude-cancel".to_string(), ConfirmOutcome::Cancelled)]);
    assert_eq!(
        steps_of(&events).last().unwrap(),
        &("send_message".to_string(), StepState::Denied, "Cancelled before it ran.".to_string())
    );
    // The process is gone, and with it any way for the tool to run.
    let pid = fs::read_to_string(dir.path().join("pid.txt")).unwrap().trim().to_string();
    let deadline = Instant::now() + Duration::from_secs(3);
    while alive(&pid) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    assert!(!alive(&pid), "the process survived the cancel");
    // (The CLI is told no and ended in the same breath, so whether it
    // still read the answer is a race nobody needs to win: a tool
    // does not start before the CLI has an answer, and this one will
    // never read a yes.)
    // The backend holds no process any more.
    assert!(agent.process.is_none());
    assert!(!answer_confirm("req-claude-cancel", ConfirmDecision::Allow));
}

// --- calls nobody is asked about -----------------------------------

#[test]
fn a_request_for_a_tool_the_ask_was_not_given_is_denied_at_once_without_asking() {
    // Three kinds: a tool that is not ticked, a tool nobody listed,
    // and a ticked read tool the CLI should not have asked about.
    for (case, tool) in [("unticked", "delete_message"), ("unknown", "wipe_everything"), ("read", "read_thread")] {
        let dir = tempfile::tempdir().unwrap();
        let body = format!(
            r#"
echo '{{"type":"control_request","request_id":"req-claude-no-{case}","request":{{"subtype":"can_use_tool","tool_name":"mcp__chat__{tool}","input":{{"message_id":"m-1"}},"tool_use_id":"toolu_x"}}}}'
read -r answer
printf '%s\n' "$answer" > "$here/answer.txt"
echo '{{"type":"user","message":{{"role":"user","content":[{{"type":"tool_result","content":"refused","is_error":true,"tool_use_id":"toolu_x"}}]}}}}'
say "I could not do that."
done_with "I could not do that."
"#
        );
        let mut agent = ClaudeAgent::new(fake_cli(dir.path(), &body), run());
        let started = Instant::now();
        let (reply, events) = send(&mut agent, LONG, |_, _| panic!("{case}: the person must not be asked"));
        assert_eq!(reply.unwrap().text, "I could not do that.", "{case}");
        assert!(started.elapsed() < Duration::from_secs(3), "{case}: it waited");
        let answer = answer_file(dir.path(), "answer.txt");
        assert_eq!(answer["response"]["response"]["behavior"], "deny", "{case}");
        assert_eq!(answer["response"]["response"]["message"], NOT_AVAILABLE, "{case}");
        assert!(confirms_of(&events).is_empty() && closed_of(&events).is_empty(), "{case}");
        assert_eq!(
            steps_of(&events),
            [(tool.to_string(), StepState::Denied, "Not available in this ask.".to_string())],
            "{case}"
        );
        let AskEvent::Step(step) = &events[0] else { panic!("{events:?}") };
        assert_eq!(step.connection, "chat", "{case}");
        assert_eq!(step.input_summary, "message_id: m-1", "{case}");
        if case != "read" {
            assert_eq!(step.kind, ToolKind::Act, "{case}");
        }
    }
}

#[test]
fn a_call_the_cli_refuses_by_itself_is_a_denied_step() {
    let dir = tempfile::tempdir().unwrap();
    let body = r#"
echo '{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_del","name":"mcp__chat__delete_message","input":{"message_id":"m-1"}}]}}'
echo '{"type":"system","subtype":"permission_denied","tool_name":"mcp__chat__delete_message","tool_use_id":"toolu_del","message":"Claude requested permissions to use mcp__chat__delete_message, but you have not granted it yet."}'
echo '{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"Permission denied","is_error":true,"tool_use_id":"toolu_del"}]},"tool_result_meta":[{"id":"toolu_del","non_execution_kind":"permission-rule"}]}'
say "I cannot delete messages from here."
done_with "I cannot delete messages from here."
"#;
    let mut agent = ClaudeAgent::new(fake_cli(dir.path(), body), run());
    let (reply, events) = send(&mut agent, LONG, |_, _| panic!("the person must not be asked"));
    assert_eq!(reply.unwrap().text, "I cannot delete messages from here.");
    assert_eq!(
        steps_of(&events),
        [
            ("delete_message".to_string(), StepState::Running, String::new()),
            ("delete_message".to_string(), StepState::Denied, "Not available in this ask.".to_string()),
        ]
    );
    // The same refusal reported only through the tool result.
    let dir = tempfile::tempdir().unwrap();
    let body = body.lines().filter(|line| !line.contains("permission_denied")).collect::<Vec<_>>().join("\n");
    let mut agent = ClaudeAgent::new(fake_cli(dir.path(), &body), run());
    let (_, events) = send(&mut agent, LONG, |_, _| panic!("the person must not be asked"));
    assert_eq!(steps_of(&events)[1], ("delete_message".to_string(), StepState::Denied, "Permission denied".to_string()));
}

#[test]
fn a_malformed_request_is_answered_safely_and_never_reaches_the_person() {
    let dir = tempfile::tempdir().unwrap();
    // No request id (nothing to answer), no tool name, an input that
    // is not an object, and a kind of request this app does not
    // know.
    let body = r#"
echo '{"type":"control_request","request":{"subtype":"can_use_tool","tool_name":"mcp__chat__send_message","input":{}}}'
echo '{"type":"control_request","request_id":"req-claude-bad-1","request":{"subtype":"can_use_tool","input":{"to":"bo@example.com"}}}'
read -r answer; printf '%s\n' "$answer" > "$here/answer-1.txt"
echo '{"type":"control_request","request_id":"req-claude-bad-2","request":{"subtype":"can_use_tool","tool_name":"mcp__chat__send_message","input":"send it"}}'
read -r answer; printf '%s\n' "$answer" > "$here/answer-2.txt"
echo '{"type":"control_request","request_id":"req-claude-bad-3","request":{"subtype":"hook_callback","callback_id":"x"}}'
read -r answer; printf '%s\n' "$answer" > "$here/answer-3.txt"
echo 'this line is not json at all'
echo '{"type":"control_request","request_id":"","request":{"subtype":"can_use_tool"}}'
say "Nothing was sent."
done_with "Nothing was sent."
"#;
    let mut agent = ClaudeAgent::new(fake_cli(dir.path(), body), run());
    let (reply, events) = send(&mut agent, LONG, |_, _| panic!("the person must not be asked"));
    assert_eq!(reply.unwrap().text, "Nothing was sent.");
    for name in ["answer-1.txt", "answer-2.txt"] {
        let answer = answer_file(dir.path(), name);
        assert_eq!(answer["response"]["subtype"], "success", "{name}");
        assert_eq!(answer["response"]["response"]["behavior"], "deny", "{name}");
    }
    let unknown = answer_file(dir.path(), "answer-3.txt");
    assert_eq!(unknown["response"]["subtype"], "error");
    assert_eq!(unknown["response"]["request_id"], "req-claude-bad-3");
    assert!(unknown["response"].get("response").is_none());
    // Nothing to show: no step, no confirm.
    assert!(steps_of(&events).is_empty() && confirms_of(&events).is_empty(), "{events:?}");
}

#[test]
fn a_built_in_tool_still_stops_the_ask_as_in_an_ask_without_tools() {
    let dir = tempfile::tempdir().unwrap();
    let body = r#"
echo '{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_sh","name":"Bash","input":{"command":"ls"}}]}}'
sleep 30
"#;
    let mut agent = ClaudeAgent::new(fake_cli(dir.path(), body), run());
    let (reply, _) = send(&mut agent, LONG, |_, _| panic!("nothing to confirm"));
    match reply {
        Err(AskError::Backend(message)) => assert!(message.contains("tried to use a tool (Bash)"), "{message}"),
        other => panic!("{other:?}"),
    }
}

// --- the run itself ------------------------------------------------

#[test]
fn a_connection_that_is_down_is_said_in_the_notice() {
    let dir = tempfile::tempdir().unwrap();
    let body = r#"
echo '{"type":"system","subtype":"init","mcp_servers":[{"name":"chat","status":"failed"}]}'
say "I could not reach the chat."
done_with "I could not reach the chat."
"#;
    let mut agent = ClaudeAgent::new(fake_cli(dir.path(), body), run());
    let (reply, _) = send(&mut agent, LONG, |_, _| panic!("nothing to confirm"));
    assert_eq!(
        reply.unwrap().notice,
        "The connection \"chat\" could not be reached, so this answer was made without it."
    );
}

#[test]
fn running_out_of_turns_is_a_plain_error() {
    let dir = tempfile::tempdir().unwrap();
    let body = r#"echo '{"type":"result","subtype":"error_max_turns","is_error":true,"num_turns":12}'"#;
    let mut agent = ClaudeAgent::new(fake_cli(dir.path(), body), run());
    let (reply, _) = send(&mut agent, LONG, |_, _| panic!("nothing to confirm"));
    match reply {
        Err(AskError::Backend(message)) => assert!(message.starts_with("The ask stopped after 12 steps"), "{message}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_claude_connection_nobody_listed_stops_the_ask_before_anything_starts() {
    let dir = tempfile::tempdir().unwrap();
    let settings = AgentSettings {
        enabled: true,
        connections: vec![AgentConnection { name: "Never listed docs".into(), ..chat(AgentConnectionKind::Claude) }],
        ..AgentSettings::default()
    };
    let run = AgentRun::new(&settings, None).unwrap();
    let mut agent = ClaudeAgent::new(fake_cli(dir.path(), READS_THE_THREAD), run);
    // Warming says nothing; the ask does.
    agent.warm(&setup(false));
    let (reply, events) = send(&mut agent, LONG, |_, _| panic!("nothing to confirm"));
    match reply {
        Err(AskError::Unavailable(message)) => {
            assert!(message.starts_with("Refresh the connections first"), "{message}");
            assert!(message.contains("Never listed docs"), "{message}");
        }
        other => panic!("{other:?}"),
    }
    assert!(events.is_empty());
    assert!(!dir.path().join("args.txt").exists(), "the CLI was started without its connection");
}

#[test]
fn a_warmed_agent_process_answers_and_reports_its_head_start() {
    let dir = tempfile::tempdir().unwrap();
    let mut agent = ClaudeAgent::new(fake_cli(dir.path(), READS_THE_THREAD), run());
    agent.warm(&setup(false));
    assert!(agent.process.is_some());
    thread::sleep(Duration::from_millis(150));
    assert!(!dir.path().join("question.txt").exists(), "nothing is sent before there is a question");
    let (reply, _) = send(&mut agent, LONG, |_, _| panic!("nothing to confirm"));
    assert!(reply.unwrap().warm_lead_ms.unwrap() >= 150);
}

// --- through the plan ----------------------------------------------

/// The whole path a caller takes: settings, plan, backend, send.
#[test]
fn an_agent_plan_runs_the_agent_backend_and_the_reply_carries_its_steps() {
    // `AskPlan::backend` hands out a stub while another test has one
    // installed; that test holds this lock for as long as it does.
    let _no_stub = crate::settings::CACHE_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let cli = fake_cli(dir.path(), READS_THE_THREAD);
    let refine = RefineSettings { claude_cli: cli, ..RefineSettings::default() };
    let ask = AskSettings::default();
    let plain = AskPlan::with_terms(&ask, &refine, &[]);
    let plan = plain.clone().with_agent(Some(run()));
    assert!(plan.agent().is_some() && plain.agent().is_none());
    // Twice the time of an ask without tools.
    assert_eq!(plan.timeout, plain.timeout * 2);
    assert_eq!(plan.routed_to(), None);
    assert!(plan.setup.system.contains("\nYour tools:\n- For this question you have tools"));
    assert!(plan.setup.system.contains("- Connection \"chat\": to look things up: read_thread; to prepare a draft: create_draft; needs the person's confirmation every time: send_message.\n"));
    assert!(!plain.setup.system.contains("Your tools"));

    let mut backend = plan.backend();
    assert_eq!(backend.id(), "claude-cli");
    let mut events = Vec::new();
    let reply = plan
        .send(backend.as_mut(), &AskInput::text("what did anna ask in the release thread"), &mut |e| events.push(e), &CancelFlag::new())
        .unwrap();
    assert_eq!(reply.text, "Anna asked to move the review to Thursday at 14:00.");
    assert_eq!(reply.steps.len(), 1);
    assert_eq!((reply.steps[0].tool.as_str(), reply.steps[0].state), ("read_thread", StepState::Done));
    assert_eq!(reply.context_source, ContextSource::None);
    assert_eq!(steps_of(&events).len(), 2);
    // The prompt the CLI got is the plan's, with the tools in it,
    // and the question went in fenced.
    let system = fs::read_to_string(dir.path().join("system.txt")).unwrap();
    assert_eq!(system, plan.setup.system);
    let sent: Value = serde_json::from_str(&fs::read_to_string(dir.path().join("question.txt")).unwrap()).unwrap();
    assert_eq!(
        sent["message"]["content"][0]["text"],
        json!("<question>\nwhat did anna ask in the release thread\n</question>")
    );

    // An OpenAI-compatible backend cannot have tools: the plan stays
    // a plain ask whatever it is handed.
    let openai = RefineSettings { backend: crate::settings::RefineBackendKind::OpenaiCompatible, ..RefineSettings::default() };
    let other = AskPlan::with_terms(&ask, &openai, &[]);
    let same = other.clone().with_agent(Some(run()));
    assert!(same.agent().is_none());
    assert_eq!(same, other);
}

/// The pin for everything that is not an agent ask: with Agent off,
/// or on with nothing that can run, the CLI is started with exactly
/// the arguments of an ask from before Agent existed.
#[test]
fn an_ask_that_is_not_an_agent_ask_starts_the_cli_exactly_as_before() {
    // See above: no stub backend while this runs.
    let _no_stub = crate::settings::CACHE_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let not_agent_asks = [
        AgentSettings::default(),
        // On, and the connection enabled, with nothing ticked.
        AgentSettings { enabled: true, connections: vec![AgentConnection { allowed_tools: Vec::new(), ..chat(AgentConnectionKind::Custom) }], ..AgentSettings::default() },
        // On, with only a send ticked, and drafts only.
        AgentSettings { enabled: true, connections: vec![AgentConnection { allowed_tools: vec![fixture::SEND_MESSAGE.into()], ..chat(AgentConnectionKind::Custom) }], ..AgentSettings::default() },
        // Everything ticked, and the switch off.
        AgentSettings { enabled: false, connections: vec![chat(AgentConnectionKind::Custom)], ..AgentSettings::default() },
    ];
    for (n, settings) in not_agent_asks.iter().enumerate() {
        let dir = tempfile::tempdir().unwrap();
        let body = r#"
say "Four."
done_with "Four."
"#;
        let refine = RefineSettings { claude_cli: fake_cli(dir.path(), body), ..RefineSettings::default() };
        let before = AskPlan::with_terms(&AskSettings::default(), &refine, &[]);
        let plan = before.clone().with_agent(AgentRun::for_ask(settings, refine.backend, None));
        assert_eq!(plan, before, "case {n}: the plan itself is unchanged");
        let mut backend = plan.backend();
        let reply = plan.send(backend.as_mut(), &AskInput::text("two plus two"), &mut |_| {}, &CancelFlag::new()).unwrap();
        assert_eq!(reply.text, "Four.");
        assert!(reply.steps.is_empty());
        let args: Vec<String> = fs::read_to_string(dir.path().join("args.txt")).unwrap().split_whitespace().map(str::to_string).collect();
        // What the plain ask builds, with the empty tool list lost to
        // the shell's `echo` and the prompt file's own name.
        let mut expected: Vec<String> = claude_cli::spawn_args(&plan.setup, Path::new("PROMPT")).into_iter().filter(|a| !a.is_empty()).collect();
        let at = expected.iter().position(|a| a == "PROMPT").unwrap();
        expected[at] = args[at].clone();
        assert_eq!(args, expected, "case {n}");
        for flag in ["--mcp-config", "--permission-prompt-tool", "--setting-sources", "--allowedTools", "--disallowedTools", "--max-turns"] {
            assert!(!args.iter().any(|a| a == flag), "case {n}: {flag}");
        }
        assert!(args.iter().any(|a| a == "--safe-mode"), "case {n}");
        assert!(!dir.path().join("mcp.json").exists(), "case {n}: no connection file");
        assert!(!fs::read_to_string(dir.path().join("system.txt")).unwrap().contains("Your tools"), "case {n}");
    }
}
