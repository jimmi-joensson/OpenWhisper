//! Tests for an ask with tools (Agent, TASK-99) as the rest of the
//! app sees it: the plan, the steps and the confirm on the snapshot,
//! what is kept of it, and where it looks for its context.
//!
//! Against the stub backend ([`super::test_support`]), which here
//! plays an agent backend: it sends the events a real one sends and
//! waits on the same gate ([`steps::Waiting`]). The real backend is
//! tested in `crate::agent::claude`. No test here starts a process
//! or calls a tool.

use std::thread;

use super::test_support::{Calls, Stub, scope};
use super::*;
use crate::agent::{AgentRun, FrontApp};
use crate::settings::{
    AgentConnection, AgentConnectionKind, AgentSettings, AppConnection, CustomServer, HistorySettings, ToolInfo,
};

fn on() -> AskSettings {
    AskSettings { enabled: true, ..AskSettings::default() }
}

fn calls() -> Arc<Mutex<Calls>> {
    Arc::new(Mutex::new(Calls::default()))
}

fn wait_until(what: &str, done: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting until {what}");
        thread::sleep(Duration::from_millis(5));
    }
}

/// A made-up chat connection: two reads, a draft, a send, a delete.
fn chat(ticked: &[&str]) -> AgentConnection {
    AgentConnection {
        name: "Team chat".into(),
        kind: AgentConnectionKind::Custom,
        enabled: true,
        allowed_tools: ticked.iter().map(|t| t.to_string()).collect(),
        tools: vec![
            ToolInfo { read_only: Some(true), ..ToolInfo::new("read_thread") },
            ToolInfo::new("create_draft"),
            ToolInfo::new("send_message"),
            ToolInfo { destructive: Some(true), ..ToolInfo::new("delete_message") },
        ],
        custom: Some(CustomServer { command: "/opt/example/chat-server".into(), ..CustomServer::default() }),
    }
}

fn agent(ticked: &[&str], drafts_only: bool) -> AgentSettings {
    AgentSettings {
        enabled: true,
        drafts_only,
        connections: vec![chat(ticked)],
        app_connections: Some(vec![AppConnection::new("com.example.chat", "Chat", "Team chat")]),
        ..AgentSettings::default()
    }
}

fn step(id: &str, tool: &str, kind: ToolKind, state: StepState, outcome: &str) -> AskStep {
    AskStep {
        id: id.into(),
        connection: "Team chat".into(),
        tool: tool.into(),
        title: tool.replace('_', " "),
        kind,
        input_summary: "to: bo@example.com".into(),
        state,
        outcome: outcome.into(),
    }
}

fn confirm(request_id: &str) -> AskConfirm {
    AskConfirm {
        request_id: request_id.into(),
        step_id: "s2".into(),
        connection: "Team chat".into(),
        tool: "send_message".into(),
        title: "Send Message".into(),
        input: "{\n  \"to\": \"bo@example.com\",\n  \"body\": \"Room confirmed\"\n}".into(),
        timeout_s: 120,
        armed_in_ms: CONFIRM_ARM_MS,
    }
}

/// A confirm as it is on the snapshot, without the countdown.
fn on_show(snap: &AskSnapshot) -> Option<AskConfirm> {
    snap.confirm.clone().map(|confirm| AskConfirm { armed_in_ms: CONFIRM_ARM_MS, ..confirm })
}

/// Wait until a yes to the confirm on show counts.
fn wait_until_armed() {
    wait_until("a yes to the confirm counts", || snapshot().confirm.is_some_and(|confirm| confirm.armed_in_ms == 0));
}

/// A factory whose backends do what an agent backend does for "send
/// Bo a message": read the thread, then call the send and wait for
/// the person, on the same gate a real backend waits on.
fn sending(
    calls: &Arc<Mutex<Calls>>,
    request_id: &'static str,
) -> impl Fn(&AskPlan) -> Box<dyn AskBackend> + Send + Sync + use<> {
    let calls = calls.clone();
    move |_| {
        Box::new(Stub::new(calls.clone(), move |_, sink, cancel| {
            sink(AskEvent::Step(step("s1", "read_thread", ToolKind::Read, StepState::Running, "")));
            sink(AskEvent::Step(step("s1", "read_thread", ToolKind::Read, StepState::Done, "Thread #release-plan")));
            sink(AskEvent::Step(step("s2", "send_message", ToolKind::Act, StepState::Waiting, "")));
            let waiting = steps::Waiting::open(request_id);
            sink(AskEvent::Confirm(confirm(request_id)));
            let deadline = Instant::now() + Duration::from_secs(5);
            let outcome = loop {
                if cancel.is_cancelled() {
                    break ConfirmOutcome::Cancelled;
                }
                match waiting.answer() {
                    Some(ConfirmDecision::Allow) => break ConfirmOutcome::Allowed,
                    Some(ConfirmDecision::Deny) => break ConfirmOutcome::Denied,
                    None => {}
                }
                assert!(Instant::now() < deadline, "the confirm was never answered");
                thread::sleep(Duration::from_millis(5));
            };
            drop(waiting);
            sink(AskEvent::ConfirmClosed { request_id: request_id.to_string(), outcome });
            match outcome {
                ConfirmOutcome::Allowed => {
                    sink(AskEvent::Step(step("s2", "send_message", ToolKind::Act, StepState::Running, "")));
                    sink(AskEvent::Step(step("s2", "send_message", ToolKind::Act, StepState::Done, "ok: message sent")));
                    sink(AskEvent::Text("I sent Bo the message.".into()));
                    Ok("I sent Bo the message.".into())
                }
                ConfirmOutcome::Denied => {
                    sink(AskEvent::Step(step("s2", "send_message", ToolKind::Act, StepState::Denied, "You said no.")));
                    sink(AskEvent::Text("The message was not sent.".into()));
                    Ok("The message was not sent.".into())
                }
                _ => Err(AskError::Cancelled),
            }
        }))
    }
}

/// Start the app's ask on a thread and wait until it is waiting for
/// the person.
fn ask_until_confirming(question: &'static str) -> (thread::JoinHandle<AskSnapshot>, AskSnapshot) {
    let asking = thread::spawn(move || submit(question).unwrap());
    wait_until("the ask waits for the person", || snapshot().phase == AskPhase::Confirming);
    (asking, snapshot())
}

// --- when an ask is an agent ask -----------------------------------

#[test]
fn the_apps_ask_has_tools_only_with_the_switch_the_claude_backend_and_a_ticked_tool() {
    let _dictation = crate::dictation::TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let calls = calls();
    let plans: Arc<Mutex<Vec<AskPlan>>> = Arc::new(Mutex::new(Vec::new()));
    let _scope = scope(on(), {
        let (calls, plans) = (calls.clone(), plans.clone());
        move |plan| {
            plans.lock().unwrap().push(plan.clone());
            Box::new(Stub::saying(calls.clone(), "Four."))
        }
    });
    let ask = || {
        assert_eq!(submit("two plus two").unwrap().phase, AskPhase::Done);
        plans.lock().unwrap().pop().expect("a plan")
    };

    // Agent as a fresh install has it: the ask from before.
    let plain = ask();
    assert!(plain.agent().is_none());
    assert_eq!(plain.timeout, Duration::from_secs(60));
    assert!(!plain.setup.system.contains("Your tools"));
    assert!(plain.setup.system.contains("you cannot send, save, delete, open"));

    // Switched on with a ticked read tool: an ask with tools.
    crate::settings::set_agent_settings_cache(agent(&["read_thread"], true));
    let with_tools = ask();
    let run = with_tools.agent().expect("an agent ask");
    assert_eq!(run.tools.allowed, ["mcp__Team_chat__read_thread"]);
    assert!(run.tools.confirm.is_empty());
    assert_eq!(with_tools.timeout, Duration::from_secs(120));
    assert!(with_tools.setup.system.contains("\nYour tools:\n"));
    assert!(!with_tools.setup.system.contains("you cannot send, save, delete, open"));
    // Everything else about the plan is the plain one's.
    assert_eq!((with_tools.backend, &with_tools.setup.model, with_tools.setup.web_search), (plain.backend, &plain.setup.model, false));

    // Each of the conditions taken away again gives the plain ask,
    // prompt and all.
    let same_as_plain = |plan: &AskPlan| plan.agent().is_none() && plan.setup.system == plain.setup.system;
    crate::settings::set_agent_settings_cache(AgentSettings { enabled: false, ..agent(&["read_thread"], true) });
    assert!(same_as_plain(&ask()), "switched off");
    crate::settings::set_agent_settings_cache(agent(&[], true));
    assert!(same_as_plain(&ask()), "nothing ticked");
    crate::settings::set_agent_settings_cache(agent(&["send_message"], true));
    assert!(same_as_plain(&ask()), "only an act tool ticked, with drafts only");
    let mut off = agent(&["read_thread"], true);
    off.connections[0].enabled = false;
    crate::settings::set_agent_settings_cache(off);
    assert!(same_as_plain(&ask()), "the connection switched off");
    crate::settings::set_agent_settings_cache(agent(&["read_thread"], true));
    crate::settings::set_refine_settings_cache(RefineSettings {
        backend: RefineBackendKind::OpenaiCompatible,
        ..RefineSettings::default()
    });
    assert!(ask().agent().is_none(), "an endpoint that cannot use connections");
}

#[test]
fn the_tools_of_an_ask_are_the_ones_on_disk_when_it_starts() {
    let _dictation = crate::dictation::TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let calls = calls();
    let plans: Arc<Mutex<Vec<AskPlan>>> = Arc::new(Mutex::new(Vec::new()));
    let _scope = scope(on(), {
        let (calls, plans) = (calls.clone(), plans.clone());
        move |plan| {
            plans.lock().unwrap().push(plan.clone());
            Box::new(Stub::saying(calls.clone(), "Four."))
        }
    });
    // The app loaded Agent with a send ticked and drafts only off.
    crate::settings::set_agent_settings_cache(agent(&["read_thread", "send_message"], false));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    crate::settings::set_settings_file(Some(&path));
    // From a terminal, the person switches drafts only back on.
    let on_disk = serde_json::json!({ "agent": agent(&["read_thread", "send_message"], true) });
    std::fs::write(&path, on_disk.to_string()).unwrap();
    submit("two plus two").unwrap();
    let plan = plans.lock().unwrap().pop().unwrap();
    let run = plan.agent().expect("still an agent ask");
    assert!(run.tools.confirm.is_empty(), "the send was taken away on disk");
    assert!(run.tools.disallowed.contains(&"mcp__Team_chat__send_message".to_string()));
    assert!(plan.setup.system.contains("Nothing that sends, posts, deletes or changes anything is available"));
    // And then switches Agent off altogether.
    std::fs::write(&path, r#"{"agent":{"enabled":false}}"#).unwrap();
    submit("two plus two").unwrap();
    assert!(plans.lock().unwrap().pop().unwrap().agent().is_none());
}

// --- steps and the confirm on the snapshot -------------------------

#[test]
fn a_confirm_shows_on_the_snapshot_and_allow_lets_the_ask_finish() {
    let _dictation = crate::dictation::TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let calls = calls();
    let _scope = scope(on(), sending(&calls, "req-session-allow"));
    let store = super::test_support::store();
    let dir = tempfile::tempdir().unwrap();
    crate::settings::set_history_settings_cache(HistorySettings {
        enabled: true,
        retention_days: 0,
        folder: Some(dir.path().to_string_lossy().into_owned()),
    });

    let (asking, waiting) = ask_until_confirming("send bo a message that the room is confirmed");
    // What the panel shows while it waits: both steps, the confirm
    // with the whole input, and that a cancel would stop something.
    assert_eq!(on_show(&waiting), Some(confirm("req-session-allow")));
    assert_eq!(
        waiting.steps.iter().map(|s| (s.tool.as_str(), s.state)).collect::<Vec<_>>(),
        [("read_thread", StepState::Done), ("send_message", StepState::Waiting)]
    );
    assert!(is_busy());
    assert!(AskPhase::Confirming.is_busy() && AskPhase::Confirming.is_running());
    assert_eq!(serde_json::to_value(waiting.phase).unwrap(), "confirming");
    let json = serde_json::to_value(&waiting).unwrap();
    assert_eq!(json["confirm"]["request_id"], "req-session-allow");
    assert_eq!(json["steps"][1]["state"], "waiting");
    assert_eq!(json["steps"][1]["kind"], "act");

    // An answer meant for another ask, or another confirm, does
    // nothing.
    assert!(!confirm_allow(waiting.id + 1, "req-session-allow"));
    assert!(!confirm_allow(waiting.id, "some-other-request"));
    assert!(!confirm_deny(waiting.id + 1, "req-session-allow"));
    assert_eq!(snapshot().phase, AskPhase::Confirming);

    // A click that lands the moment the confirm comes up was not
    // meant for it: it is refused, it is not a no, and the confirm
    // stays on show. The snapshot says how long until a yes counts.
    let left = snapshot().confirm.expect("on show").armed_in_ms;
    assert!(left > 0 && left <= CONFIRM_ARM_MS, "{left}");
    assert_eq!(serde_json::to_value(snapshot()).unwrap()["confirm"]["armed_in_ms"].as_u64().map(|ms| ms > 0), Some(true));
    assert!(!confirm_allow(waiting.id, "req-session-allow"), "a yes within the first moment must not count");
    assert!(!confirm_allow(waiting.id, "req-session-allow"));
    let still = snapshot();
    assert_eq!(still.phase, AskPhase::Confirming);
    assert_eq!(on_show(&still), Some(confirm("req-session-allow")));
    assert_eq!(still.steps[1].state, StepState::Waiting);
    // The countdown runs without the snapshot's version changing.
    thread::sleep(Duration::from_millis(60));
    let later = snapshot();
    assert!(later.confirm.as_ref().unwrap().armed_in_ms < left, "{left}");
    assert_eq!(later.version, still.version);

    wait_until_armed();
    assert!(confirm_allow(waiting.id, "req-session-allow"));
    // At once: the confirm is gone from the snapshot, and a second
    // click finds nothing.
    let after = snapshot();
    assert_eq!(after.confirm, None);
    assert_ne!(after.phase, AskPhase::Confirming);
    assert!(!confirm_allow(waiting.id, "req-session-allow"));
    assert!(!confirm_deny(waiting.id, "req-session-allow"));

    let done = asking.join().unwrap();
    assert_eq!(done.phase, AskPhase::Done);
    assert_eq!(done.answer, "I sent Bo the message.");
    assert_eq!(done.confirm, None);
    assert_eq!(
        done.steps.iter().map(|s| (s.tool.as_str(), s.state, s.outcome.as_str())).collect::<Vec<_>>(),
        [("read_thread", StepState::Done, "Thread #release-plan"), ("send_message", StepState::Done, "ok: message sent")]
    );

    // The diagnostics row counts the calls, and nothing more.
    let row = runs::recent(store, 1).unwrap().remove(0);
    assert_eq!((row.status.as_str(), row.tool_calls, row.context_source.as_deref()), ("answered", Some(2), Some("none")));
    // The history entry lists the tools and how each ended, never
    // what they were given.
    let page = crate::history::list(dir.path(), "", 10, 0);
    assert_eq!(page.stored, 1);
    let tools: Vec<(&str, &str, &str)> =
        page.entries[0].tools.iter().map(|t| (t.connection.as_str(), t.tool.as_str(), t.outcome.as_str())).collect();
    assert_eq!(tools, [("Team chat", "read_thread", "done"), ("Team chat", "send_message", "done")]);
    let file = std::fs::read_to_string(dir.path().join(format!("{}/{}.md", &page.entries[0].id[..4], {
        let id = &page.entries[0].id;
        format!("{}-{}-{}", &id[..4], &id[4..6], &id[6..8])
    })))
    .unwrap();
    assert!(file.contains("<!-- extra: kind=ask; tools=2 -->"), "{file}");
    assert!(file.contains("> Tools: read_thread in Team chat: done\n> send_message in Team chat: done\n"), "{file}");
    assert!(!file.contains("bo@example.com") && !file.contains("Room confirmed"), "{file}");
    crate::settings::set_history_settings_cache(HistorySettings::default());
}

#[test]
fn a_denied_confirm_ends_the_step_as_denied_and_the_ask_still_answers() {
    let _dictation = crate::dictation::TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let calls = calls();
    let _scope = scope(on(), sending(&calls, "req-session-deny"));
    let (asking, waiting) = ask_until_confirming("send bo a message that the room is confirmed");
    assert!(confirm_deny(waiting.id, "req-session-deny"));
    assert!(!confirm_allow(waiting.id, "req-session-deny"), "the no stands");
    let done = asking.join().unwrap();
    assert_eq!((done.phase, done.answer.as_str()), (AskPhase::Done, "The message was not sent."));
    assert_eq!(done.confirm, None);
    assert_eq!((done.steps[1].state, done.steps[1].outcome.as_str()), (StepState::Denied, "You said no."));
}

#[test]
fn a_confirm_that_runs_out_of_time_closes_by_itself() {
    let _dictation = crate::dictation::TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let calls = calls();
    let seen: Arc<Mutex<Vec<AskSnapshot>>> = Arc::new(Mutex::new(Vec::new()));
    let _scope = scope(on(), {
        let (calls, seen) = (calls.clone(), seen.clone());
        move |_| {
            let seen = seen.clone();
            Box::new(Stub::new(calls.clone(), move |_, sink, _| {
                sink(AskEvent::Text("Let me send that.".into()));
                sink(AskEvent::Step(step("s2", "send_message", ToolKind::Act, StepState::Waiting, "")));
                sink(AskEvent::Confirm(confirm("req-session-timeout")));
                seen.lock().unwrap().push(snapshot());
                // Nobody answers.
                sink(AskEvent::ConfirmClosed { request_id: "req-session-timeout".into(), outcome: ConfirmOutcome::TimedOut });
                seen.lock().unwrap().push(snapshot());
                sink(AskEvent::Step(step("s2", "send_message", ToolKind::Act, StepState::Denied, "No answer in time.")));
                sink(AskEvent::Text(" It was not sent.".into()));
                Ok("Let me send that. It was not sent.".into())
            }))
        }
    });
    let done = submit("send bo a message").unwrap();
    let seen = seen.lock().unwrap();
    assert_eq!((seen[0].phase, seen[0].confirm.is_some()), (AskPhase::Confirming, true));
    // Text had already arrived, so it goes back to answering.
    assert_eq!((seen[1].phase, seen[1].confirm.is_some()), (AskPhase::Answering, false));
    assert_eq!(done.phase, AskPhase::Done);
    assert_eq!(done.steps[0].state, StepState::Denied);
}

#[test]
fn cancel_and_dismiss_while_a_confirm_waits_refuse_it() {
    let _dictation = crate::dictation::TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let calls = calls();
    let _scope = scope(on(), sending(&calls, "req-session-cancel"));
    let store = super::test_support::store();

    // The cancel key.
    let (asking, waiting) = ask_until_confirming("send bo a message that the room is confirmed");
    assert!(cancel());
    let stopped = snapshot();
    assert_eq!(stopped.phase, AskPhase::Cancelled);
    assert_eq!(stopped.confirm, None);
    // The call that waited did not run, and says so.
    assert_eq!(stopped.steps[1].state, StepState::Denied);
    assert!(stopped.steps[1].outcome.contains("not done"), "{}", stopped.steps[1].outcome);
    assert_eq!(stopped.steps[0].state, StepState::Done);
    // A yes that arrives now is too late, by either door.
    assert!(!confirm_allow(waiting.id, "req-session-cancel"));
    let ended = asking.join().unwrap();
    assert_eq!(ended.phase, AskPhase::Cancelled);
    assert!(!answer_confirm("req-session-cancel", ConfirmDecision::Allow));
    assert!(!is_busy());
    // The row says it was cancelled after two calls were made.
    let row = runs::recent(store, 1).unwrap().remove(0);
    assert_eq!((row.status.as_str(), row.tool_calls), ("cancelled", Some(2)));

    // Closing the panel.
    let (asking, waiting) = ask_until_confirming("send bo a message that the room is confirmed");
    assert!(dismiss_if(waiting.id));
    assert_eq!(snapshot().phase, AskPhase::Idle);
    assert!(snapshot().steps.is_empty() && snapshot().confirm.is_none());
    assert!(!confirm_allow(waiting.id, "req-session-cancel"));
    asking.join().unwrap();
    assert!(!answer_confirm("req-session-cancel", ConfirmDecision::Allow));
    assert_eq!(calls.lock().unwrap().dropped, 2, "both backends were ended");
}

#[test]
fn an_ask_that_fails_midway_leaves_no_step_running_or_waiting() {
    let _dictation = crate::dictation::TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let calls = calls();
    let _scope = scope(on(), {
        let calls = calls.clone();
        move |_| {
            Box::new(Stub::new(calls.clone(), |_, sink, _| {
                sink(AskEvent::Step(step("s1", "read_thread", ToolKind::Read, StepState::Running, "")));
                Err(AskError::Backend("Claude Code exited without an answer".into()))
            }))
        }
    });
    let store = super::test_support::store();
    let failed = submit("what did anna ask").unwrap();
    assert_eq!(failed.phase, AskPhase::Error);
    assert_eq!(failed.steps.len(), 1);
    assert_eq!(failed.steps[0].state, StepState::Failed);
    assert!(failed.steps[0].outcome.contains("not known"), "{}", failed.steps[0].outcome);
    let row = runs::recent(store, 1).unwrap().remove(0);
    assert_eq!((row.status.as_str(), row.tool_calls), ("failed", Some(1)));
}

/// The one entry in the history folder `dir`, with its file's text.
fn only_entry(dir: &std::path::Path) -> (crate::history::HistoryEntry, String) {
    let page = crate::history::list(dir, "", 10, 0);
    assert_eq!(page.stored, 1, "{:?}", page.entries);
    let entry = page.entries[0].clone();
    let id = &entry.id;
    let file = std::fs::read_to_string(dir.join(format!("{}/{}-{}-{}.md", &id[..4], &id[..4], &id[4..6], &id[6..8]))).unwrap();
    (entry, file)
}

fn history_on() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    crate::settings::set_history_settings_cache(HistorySettings {
        enabled: true,
        retention_days: 0,
        folder: Some(dir.path().to_string_lossy().into_owned()),
    });
    dir
}

#[test]
fn an_ask_in_which_a_tool_ran_is_kept_in_history_also_when_it_did_not_finish() {
    let _dictation = crate::dictation::TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let calls = calls();
    let tools_of = |entry: &crate::history::HistoryEntry| -> Vec<(String, String)> {
        entry.tools.iter().map(|t| (t.tool.clone(), t.outcome.clone())).collect()
    };

    // It failed while a draft was being saved, with some of the
    // answer already there.
    {
        let _scope = scope(on(), {
            let calls = calls.clone();
            move |_| {
                Box::new(Stub::new(calls.clone(), |_, sink, _| {
                    sink(AskEvent::Step(step("s1", "read_thread", ToolKind::Read, StepState::Done, "Thread #release-plan")));
                    sink(AskEvent::Text("Let me prepare that.".into()));
                    sink(AskEvent::Step(step("s2", "create_draft", ToolKind::Draft, StepState::Running, "")));
                    Err(AskError::Backend("Claude Code exited without an answer".into()))
                }))
            }
        });
        let dir = history_on();
        let failed = submit("draft a reply to anna").unwrap();
        assert_eq!(failed.phase, AskPhase::Error);
        let (entry, file) = only_entry(dir.path());
        assert!(entry.is_ask());
        assert_eq!(entry.text, "draft a reply to anna");
        assert_eq!(
            entry.answer.as_deref(),
            Some("Let me prepare that.\n\n(Incomplete. The ask did not finish: Claude Code exited without an answer)")
        );
        // The read is done. Of the draft nothing more is known: it
        // may have been saved.
        assert_eq!(tools_of(&entry), [("read_thread".into(), "done".into()), ("create_draft".into(), "result_not_known".into())]);
        assert!(file.contains("<!-- extra: kind=ask; tools=2 -->"), "{file}");
        assert!(file.contains("> Tools: read_thread in Team chat: done\n> create_draft in Team chat: result_not_known\n"), "{file}");
        assert!(!file.contains("bo@example.com"), "{file}");
    }

    // It was cancelled while the send waited for a yes, after a
    // read: the read is kept, the send did not run.
    {
        let _scope = scope(on(), sending(&calls, "req-session-history"));
        let dir = history_on();
        let (asking, _) = ask_until_confirming("send bo a message that the room is confirmed");
        assert!(cancel());
        assert_eq!(asking.join().unwrap().phase, AskPhase::Cancelled);
        let (entry, _) = only_entry(dir.path());
        assert_eq!(entry.answer.as_deref(), Some("(No answer. The ask was cancelled before it finished.)"));
        assert_eq!(tools_of(&entry), [("read_thread".into(), "done".into()), ("send_message".into(), "denied".into())]);
    }

    // It ran out of time after a tool had failed.
    {
        let _scope = scope(on(), {
            let calls = calls.clone();
            move |_| {
                Box::new(Stub::new(calls.clone(), |_, sink, _| {
                    sink(AskEvent::Step(step("s1", "read_thread", ToolKind::Read, StepState::Failed, "The chat did not answer.")));
                    Err(AskError::Timeout)
                }))
            }
        });
        let dir = history_on();
        assert_eq!(submit("what did anna ask").unwrap().phase, AskPhase::Error);
        let (entry, _) = only_entry(dir.path());
        let answer = entry.answer.as_deref().unwrap();
        assert!(answer.starts_with("(No answer. The ask did not finish: No answer within ") && answer.ends_with(" seconds.)"), "{answer}");
        assert_eq!(tools_of(&entry), [("read_thread".into(), "failed".into())]);
    }

    // Nothing ran: a call that was refused, and an ask that failed
    // before any call. Neither leaves an entry, as before.
    for refused in [true, false] {
        let _scope = scope(on(), {
            let calls = calls.clone();
            move |_| {
                Box::new(Stub::new(calls.clone(), move |_, sink, _| {
                    if refused {
                        sink(AskEvent::Step(step("s1", "delete_message", ToolKind::Act, StepState::Denied, "Not available in this ask.")));
                    }
                    sink(AskEvent::Text("I could".into()));
                    Err(AskError::Backend("Claude Code exited without an answer".into()))
                }))
            }
        });
        let dir = history_on();
        assert_eq!(submit("delete the last message").unwrap().phase, AskPhase::Error);
        assert_eq!(crate::history::list(dir.path(), "", 10, 0).stored, 0, "refused: {refused}");
    }
    crate::settings::set_history_settings_cache(HistorySettings::default());
}

#[test]
fn an_ask_without_tools_records_no_calls_and_an_entry_without_them_is_as_before() {
    let _dictation = crate::dictation::TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let calls = calls();
    let _scope = scope(on(), {
        let calls = calls.clone();
        move |_| Box::new(Stub::saying(calls.clone(), "Four."))
    });
    let store = super::test_support::store();
    let dir = tempfile::tempdir().unwrap();
    crate::settings::set_history_settings_cache(HistorySettings {
        enabled: true,
        retention_days: 0,
        folder: Some(dir.path().to_string_lossy().into_owned()),
    });
    let done = submit("two plus two").unwrap();
    assert!(done.steps.is_empty() && done.confirm.is_none());
    assert_eq!(done.context_source, ContextSource::None);
    let row = runs::recent(store, 1).unwrap().remove(0);
    assert_eq!((row.tool_calls, row.context_source.as_deref()), (Some(0), Some("none")));
    let page = crate::history::list(dir.path(), "", 10, 0);
    assert!(page.entries[0].tools.is_empty());
    let id = &page.entries[0].id;
    let file = std::fs::read_to_string(dir.path().join(format!("{}/{}-{}-{}.md", &id[..4], &id[..4], &id[4..6], &id[6..8]))).unwrap();
    assert!(file.contains("<!-- extra: kind=ask -->\n"), "{file}");
    assert!(!file.contains("tools") && !file.contains("Tools"), "{file}");
    crate::settings::set_history_settings_cache(HistorySettings::default());
}

// --- the connection before the screen ------------------------------

fn routed_plan(agent: &AgentSettings, front: Option<(&str, &str)>) -> AskPlan {
    let front = front.map(|(id, name)| FrontApp::new(id, name));
    AskPlan::with_terms(&on(), &RefineSettings::default(), &[]).with_agent(AgentRun::new(agent, front.as_ref()))
}

#[test]
fn a_plan_is_pointed_at_the_connection_of_the_app_in_front() {
    let settings = agent(&["read_thread"], true);
    let routed = routed_plan(&settings, Some(("com.example.chat", "Chat")));
    assert_eq!(routed.routed_to(), Some("Team chat"));
    assert!(routed
        .setup
        .system
        .contains("The app the person is working in right now is Chat, and what is in it is reachable through the connection \"Team chat\""));
    // Another app, or none: the tools are there, the pointer is not.
    for front in [Some(("com.example.editor", "Editor")), None] {
        let plan = routed_plan(&settings, front);
        assert!(plan.agent().is_some());
        assert_eq!(plan.routed_to(), None);
        assert!(!plan.setup.system.contains("working in right now"));
    }
    // With "app in front" switched off for asks, the connection is
    // pointed at and the app is not named.
    let quiet = AskSettings { context: crate::settings::ContextSettings { front_app: false, ..Default::default() }, ..on() };
    let front = FrontApp::new("com.example.chat", "Chat");
    let plan = AskPlan::with_terms(&quiet, &RefineSettings::default(), &[]).with_agent(AgentRun::new(&settings, Some(&front)));
    assert_eq!(plan.routed_to(), Some("Team chat"));
    assert!(!plan.setup.system.contains("is Chat"), "{}", plan.setup.system);
}

#[cfg(feature = "vision")]
mod with_a_screen {
    use super::*;
    use crate::settings::VisionTrigger;
    use crate::vision::test_support::{Fake, install};

    fn vision(trigger: VisionTrigger) -> VisionSettings {
        VisionSettings { trigger, phrases: None, never_capture: Some(Vec::new()) }
    }

    /// One ask outside the app's session, against a stub that
    /// answers "Thursday."; the reply, what the sink saw, and the
    /// prompt and the image count the backend got.
    fn one_shot(plan: &AskPlan, question: &str) -> (AskReply, Vec<AskEvent>, String, usize) {
        let calls = calls();
        let mut backend = Stub::saying(calls.clone(), "Thursday.");
        let mut events = Vec::new();
        let reply = plan.send(&mut backend, &AskInput::text(question), &mut |e| events.push(e), &CancelFlag::new()).unwrap();
        let c = calls.lock().unwrap();
        (reply, events, c.last_system.clone(), c.last_images)
    }

    fn plan(trigger: VisionTrigger, choice: VisionChoice, front: Option<(&str, &str)>) -> AskPlan {
        routed_plan(&agent(&["read_thread"], true), front).with_vision(&vision(trigger), choice)
    }

    const CHAT: Option<(&str, &str)> = Some(("com.example.chat", "Chat"));
    const EDITOR: Option<(&str, &str)> = Some(("com.example.editor", "Editor"));

    #[test]
    fn a_mapped_app_takes_no_screenshot_whatever_the_trigger_says() {
        let _lock = crate::settings::CACHE_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        for trigger in VisionTrigger::ALL {
            let screen = install(Fake::default());
            let plan = plan(trigger, VisionChoice::Settings, CHAT);
            let (reply, events, system, images) = one_shot(&plan, "what did anna ask in this thread");
            assert_eq!(screen.0.captures(), 0, "{trigger:?}");
            assert!(screen.0.calls().is_empty(), "{trigger:?}: the screen was not even looked at");
            assert_eq!((reply.screen, images), (ScreenState::None, 0), "{trigger:?}");
            assert_eq!(reply.context_source, ContextSource::Connection { name: "Team chat".into() }, "{trigger:?}");
            // The sink heard where the ask looks, before anything else.
            assert_eq!(events[0], AskEvent::Context(ContextSource::Connection { name: "Team chat".into() }), "{trigger:?}");
            assert!(!events.iter().any(|e| matches!(e, AskEvent::Screen { .. })), "{trigger:?}");
            // The model is never taught to ask for the screen here:
            // "smart" is a default, the connection the better source.
            assert!(!system.contains("NEED_SCREEN"), "{trigger:?}");
            assert!(system.contains("takes the place of seeing their screen"), "{trigger:?}");
        }
    }

    #[test]
    fn an_explicit_screen_phrase_still_takes_the_screenshot_next_to_the_connection() {
        let _lock = crate::settings::CACHE_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        for trigger in [VisionTrigger::Phrases, VisionTrigger::Smart, VisionTrigger::Always] {
            let screen = install(Fake::default());
            let plan = plan(trigger, VisionChoice::Settings, CHAT);
            let (reply, events, system, images) = one_shot(&plan, "look at my screen and tell me what anna asked");
            assert_eq!(screen.0.captures(), 1, "{trigger:?}");
            assert_eq!((reply.screen, images), (ScreenState::Included, 1), "{trigger:?}");
            // Both: the screenshot went, and the ask is still pointed
            // at the connection.
            assert_eq!(reply.context_source, ContextSource::Connection { name: "Team chat".into() }, "{trigger:?}");
            assert!(events.contains(&AskEvent::Screen { state: ScreenState::Included, notice: String::new() }));
            // One prompt for the ask with and without the phrase, so
            // the warmed backend answers either way.
            assert_eq!(system, plan.setup.system, "{trigger:?}");
            assert!(system.contains("A screenshot of the person's screen may be attached"), "{trigger:?}");
        }
        // Vision switched off is off: a phrase changes nothing.
        let screen = install(Fake::default());
        let off = plan(VisionTrigger::Off, VisionChoice::Settings, CHAT);
        let (reply, _, _, images) = one_shot(&off, "look at my screen and tell me what anna asked");
        assert_eq!((screen.0.captures(), reply.screen, images), (0, ScreenState::None, 0));
        drop(screen);
        // A caller that forces the screen gets it.
        let screen = install(Fake::default());
        let forced = plan(VisionTrigger::Off, VisionChoice::Force, CHAT);
        let (reply, _, _, _) = one_shot(&forced, "what did anna ask");
        assert_eq!((screen.0.captures(), reply.screen), (1, ScreenState::Included));
        // And one that never looks at the screen does not.
        drop(screen);
        let screen = install(Fake::default());
        let never = plan(VisionTrigger::Always, VisionChoice::None, CHAT);
        let (reply, _, _, _) = one_shot(&never, "look at my screen");
        assert_eq!((screen.0.captures(), reply.screen), (0, ScreenState::None));
    }

    #[test]
    fn an_app_without_a_connection_follows_the_vision_trigger_as_before() {
        let _lock = crate::settings::CACHE_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        // Every ask carries the screen.
        let screen = install(Fake::default());
        let always = plan(VisionTrigger::Always, VisionChoice::Settings, EDITOR);
        let (reply, events, _, images) = one_shot(&always, "what did anna ask");
        assert_eq!((screen.0.captures(), reply.screen, images), (1, ScreenState::Included, 1));
        assert_eq!(reply.context_source, ContextSource::Screen);
        assert!(!events.iter().any(|e| matches!(e, AskEvent::Context(_))));
        drop(screen);
        // Phrases: only with one.
        let screen = install(Fake::default());
        let phrases = plan(VisionTrigger::Phrases, VisionChoice::Settings, EDITOR);
        let (reply, _, _, _) = one_shot(&phrases, "what did anna ask");
        assert_eq!((screen.0.captures(), reply.screen), (0, ScreenState::None));
        assert_eq!(reply.context_source, ContextSource::None);
        let (reply, _, _, _) = one_shot(&phrases, "what is on my screen");
        assert_eq!((screen.0.captures(), reply.screen), (1, ScreenState::Included));
        assert_eq!(reply.context_source, ContextSource::Screen);
        // The same prompt about the screen as an ask without tools.
        let without_tools =
            AskPlan::with_terms(&on(), &RefineSettings::default(), &[]).with_vision(&vision(VisionTrigger::Phrases), VisionChoice::Settings);
        assert_eq!(phrases.screen_prompt(false), without_tools.screen_prompt(false));
        assert_eq!(phrases.screen_prompt(true), without_tools.screen_prompt(true));
    }

    #[test]
    fn a_stand_in_for_the_screen_is_taken_in_place_of_a_capture() {
        let _lock = crate::settings::CACHE_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let screen = install(Fake::default());
        let stand_in = screen::StandIn::new(AskImage { media_type: "image/png".into(), data: vec![7; 64] });
        let plan = plan(VisionTrigger::Phrases, VisionChoice::Settings, EDITOR).with_screen_stand_in(stand_in.clone());
        let (reply, events, _, images) = one_shot(&plan, "what is on my screen");
        assert_eq!((stand_in.taken(), images, reply.screen), (1, 1, ScreenState::Included));
        assert_eq!(reply.context_source, ContextSource::Screen);
        assert!(events.contains(&AskEvent::Screen { state: ScreenState::Capturing, notice: String::new() }));
        // The screen itself was left alone.
        assert_eq!(screen.0.captures(), 0);
        assert!(screen.0.calls().is_empty());
        // No phrase, no stand-in either.
        let (reply, _, _, _) = one_shot(&plan, "two plus two");
        assert_eq!((stand_in.taken(), reply.screen), (1, ScreenState::None));
    }
}
