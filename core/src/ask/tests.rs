//! Tests for the plan of an ask and for the one ask of the running
//! app, against the stub backend ([`super::test_support`]). The real
//! backends are tested in their own files.

use std::thread;

use super::test_support::{Calls, Stub, scope};
use super::*;
use crate::settings::HistorySettings;

fn on() -> AskSettings {
    AskSettings { enabled: true, ..AskSettings::default() }
}

fn calls() -> Arc<Mutex<Calls>> {
    Arc::new(Mutex::new(Calls::default()))
}

/// A factory whose backends all answer `answer`.
fn saying(
    calls: &Arc<Mutex<Calls>>,
    answer: &'static str,
) -> impl Fn(&AskPlan) -> Box<dyn AskBackend> + Send + Sync + use<> {
    let calls = calls.clone();
    move |_| Box::new(Stub::saying(calls.clone(), answer))
}

/// A factory whose backends say "Partial", then wait until they are
/// cancelled (or give up after five seconds).
fn waiting(calls: &Arc<Mutex<Calls>>) -> impl Fn(&AskPlan) -> Box<dyn AskBackend> + Send + Sync + use<> {
    let calls = calls.clone();
    move |_| {
        Box::new(Stub::new(calls.clone(), |_, sink, cancel| {
            sink(AskEvent::Text("Partial".into()));
            let deadline = Instant::now() + Duration::from_secs(5);
            while !cancel.is_cancelled() {
                assert!(Instant::now() < deadline, "the stub was never cancelled");
                thread::sleep(Duration::from_millis(5));
            }
            Err(AskError::Cancelled)
        }))
    }
}

fn wait_until(what: &str, done: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting until {what}");
        thread::sleep(Duration::from_millis(5));
    }
}

// --- the plan -------------------------------------------------------

#[test]
fn the_default_plan_is_sonnet_at_low_effort_without_tools() {
    let plan = AskPlan::with_terms(&on(), &RefineSettings::default(), &[]);
    assert_eq!(plan.backend, RefineBackendKind::ClaudeCli);
    assert_eq!(plan.setup.model, "sonnet");
    assert_eq!(plan.setup.effort, Some(RefineEffort::Low));
    assert_eq!(plan.effort_id(), "low");
    assert!(!plan.setup.web_search);
    assert_eq!(plan.timeout, Duration::from_secs(60));
    assert_eq!(plan.notice, "");
    assert!(plan.setup.system.contains("You cannot look anything up"));
    assert!(plan.setup.system.contains("Today is "));
    assert_eq!(plan.destination(), crate::refine::claude_cli::DESTINATION);
}

#[test]
fn web_search_doubles_the_timeout_and_changes_the_prompt() {
    let ask = AskSettings { web_search: true, instructions: "Answer in Danish.".into(), ..on() };
    let plan = AskPlan::with_terms(&ask, &RefineSettings::default(), &["Brennholt".to_string()]);
    assert!(plan.setup.web_search);
    assert_eq!(plan.timeout, Duration::from_secs(120));
    assert!(plan.setup.system.contains("You can search the web"));
    assert!(plan.setup.system.contains("Brennholt"));
    assert!(plan.setup.system.ends_with("Answer in Danish."));
    // The longest timeout a user can set still doubles.
    let slow = AskSettings { timeout_ms: 9_999_999, ..ask };
    assert_eq!(AskPlan::with_terms(&slow, &RefineSettings::default(), &[]).timeout, Duration::from_secs(600));
}

#[test]
fn the_openai_backend_uses_refines_model_and_has_no_web_search() {
    let refine = RefineSettings {
        backend: RefineBackendKind::OpenaiCompatible,
        openai: OpenAiSettings {
            base_url: "http://localhost:11434/v1".into(),
            model: "local-model".into(),
            effort: None,
        },
        ..RefineSettings::default()
    };
    let ask = AskSettings { web_search: true, model: Some("opus".into()), effort: Some(RefineEffort::High), ..on() };
    let plan = AskPlan::with_terms(&ask, &refine, &[]);
    assert_eq!(plan.backend, RefineBackendKind::OpenaiCompatible);
    assert_eq!(plan.setup.model, "local-model");
    assert_eq!((plan.setup.effort, plan.effort_id()), (None, ""));
    assert!(!plan.setup.web_search);
    assert_eq!(plan.notice, WEB_SEARCH_UNAVAILABLE);
    // No search, so no doubled timeout either.
    assert_eq!(plan.timeout, Duration::from_secs(60));
    assert!(plan.setup.system.contains("You cannot look anything up"));
    assert_eq!(plan.destination(), "the server at localhost:11434");
}

#[test]
fn status_says_what_an_ask_would_use_without_calling_a_model() {
    let refine = RefineSettings {
        claude_cli: ClaudeCliSettings { path: Some("/nonexistent/claude".into()), ..ClaudeCliSettings::default() },
        ..RefineSettings::default()
    };
    let ask = AskSettings { web_search: true, model: Some("opus".into()), timeout_ms: 1, ..AskSettings::default() };
    let s = status(&ask, &refine);
    assert!(!s.settings.enabled);
    assert_eq!(s.settings.timeout_ms, crate::settings::ASK_TIMEOUT_MIN_MS);
    assert_eq!((s.model.as_str(), s.effort.as_str()), ("opus", "low"));
    assert!(s.web_search);
    assert_eq!(s.timeout_ms, 10_000);
    assert_eq!(s.backend.state, crate::refine::BackendState::NotInstalled);
    assert_eq!(s.backend.destination, crate::refine::claude_cli::DESTINATION);
    // What the shell gets is plain JSON.
    let json = serde_json::to_value(&s).unwrap();
    assert_eq!(json["backend"]["state"], "not-installed");
    assert_eq!(json["settings"]["web_search"], true);
}

// --- the plain function ---------------------------------------------

#[test]
fn answer_streams_through_the_sink_and_touches_no_global_state() {
    let calls = calls();
    // Ask is off in the settings: an explicit call is its own opt-in.
    let _scope = scope(AskSettings::default(), saying(&calls, "Four."));
    let before = snapshot();
    let mut events = Vec::new();
    let reply = answer(
        &AskSettings::default(),
        &RefineSettings::default(),
        &AskInput::text(" what is two plus two "),
        &mut |e| events.push(e),
        &CancelFlag::new(),
    )
    .unwrap();
    assert_eq!(reply.text, "Four.");
    assert_eq!(events, vec![AskEvent::Text("Fo".into()), AskEvent::Text("ur.".into())]);
    let c = calls.lock().unwrap();
    assert_eq!(c.sent, ["<question>\nwhat is two plus two\n</question>"]);
    assert_eq!((c.made, c.warmed, c.dropped), (1, 0, 1));
    assert!(c.last_system.contains("a speech recognizer wrote it down"));
    drop(c);
    assert_eq!(snapshot(), before);
    assert!(!is_busy());
}

#[test]
fn answer_refuses_an_empty_question_and_an_empty_answer() {
    let calls = calls();
    let _scope = scope(on(), saying(&calls, "   "));
    let ask = |text: &str| answer(&on(), &RefineSettings::default(), &AskInput::text(text), &mut |_| {}, &CancelFlag::new());
    assert_eq!(ask("  \n "), Err(AskError::Backend(NO_QUESTION.into())));
    assert!(calls.lock().unwrap().sent.is_empty());
    assert_eq!(ask("anyone there"), Err(AskError::Backend(NO_ANSWER.into())));
}

#[test]
fn the_plans_notice_ends_up_on_the_reply() {
    let calls = calls();
    let _scope = scope(on(), saying(&calls, "Oslo."));
    let refine = RefineSettings {
        backend: RefineBackendKind::OpenaiCompatible,
        openai: OpenAiSettings { model: "m".into(), ..OpenAiSettings::default() },
        ..RefineSettings::default()
    };
    let ask = AskSettings { web_search: true, ..on() };
    let reply = answer(&ask, &refine, &AskInput::text("capital of norway"), &mut |_| {}, &CancelFlag::new()).unwrap();
    assert_eq!(reply.notice, WEB_SEARCH_UNAVAILABLE);
}

// --- the one ask of the running app ---------------------------------

#[test]
fn with_ask_switched_off_nothing_starts_and_the_result_says_so() {
    let calls = calls();
    let _scope = scope(AskSettings::default(), saying(&calls, "Four."));
    for result in [mark_listening().map(|_| ()), begin_listening().map(|_| ()), submit("two plus two").map(|_| ())] {
        match result {
            Err(AskError::Unavailable(m)) => assert!(m.contains("switched off"), "{m}"),
            other => panic!("expected Unavailable, got {other:?}"),
        }
    }
    assert!(matches!(submit_listening(7, "two plus two", 0), Err(AskError::Unavailable(_))));
    assert_eq!(snapshot().phase, AskPhase::Idle);
    assert!(!is_busy());
    assert_eq!(calls.lock().unwrap().made, 0);
}

#[test]
fn a_spoken_ask_goes_from_listening_through_answering_to_done() {
    let calls = calls();
    let seen: Arc<Mutex<Vec<AskSnapshot>>> = Arc::new(Mutex::new(Vec::new()));
    let _scope = scope(on(), {
        let (calls, seen) = (calls.clone(), seen.clone());
        move |_| {
            let seen = seen.clone();
            Box::new(Stub::new(calls.clone(), move |_, sink, _| {
                seen.lock().unwrap().push(snapshot());
                sink(AskEvent::Text("Four".into()));
                seen.lock().unwrap().push(snapshot());
                sink(AskEvent::Text(".".into()));
                Ok("Four.".into())
            }))
        }
    });
    let version = snapshot().version;

    let id = begin_listening().unwrap();
    let listening = snapshot();
    assert_eq!((listening.id, listening.phase), (id, AskPhase::Listening));
    assert_eq!((listening.backend.as_str(), listening.model.as_str(), listening.effort.as_str()), ("claude-cli", "sonnet", "low"));
    assert!(!listening.web_search);
    assert!(listening.started_at_ms > 0);
    assert!(listening.version > version);
    assert!(is_busy());
    assert_eq!(finished_answer(), None);
    assert_eq!(calls.lock().unwrap().warmed, 1);

    let done = submit_listening(id, " what is two plus two ", 2100).unwrap();
    assert_eq!((done.id, done.phase), (id, AskPhase::Done));
    assert_eq!(done.question, "what is two plus two");
    assert_eq!(done.answer, "Four.");
    assert_eq!(done.error, "");
    assert_eq!(done.backend, "stub");
    assert!(done.first_text_ms.is_some());
    assert_eq!(done.usage.as_ref().map(|u| u.tokens_out), Some(40));
    assert_eq!(done.started_at_ms, listening.started_at_ms);
    assert!(done.version > listening.version);
    assert_eq!(done, snapshot());
    assert!(!is_busy());
    assert_eq!(finished_answer().as_deref(), Some("Four."));

    // What the panel saw on the way.
    let seen = seen.lock().unwrap();
    assert_eq!((seen[0].phase, seen[0].answer.as_str()), (AskPhase::Thinking, ""));
    assert_eq!(seen[0].question, "what is two plus two");
    assert_eq!((seen[1].phase, seen[1].answer.as_str()), (AskPhase::Answering, "Four"));
    assert!(seen[1].version > seen[0].version);

    // The backend warmed during the recording took the question, and
    // the conversation ended with the answer.
    let c = calls.lock().unwrap();
    assert_eq!((c.made, c.warmed, c.dropped), (1, 1, 1));
    assert_eq!(c.sent, ["<question>\nwhat is two plus two\n</question>"]);
}

#[test]
fn typed_text_is_asked_without_a_recording() {
    let calls = calls();
    let _scope = scope(on(), saying(&calls, "The capital of Norway is Oslo."));
    let done = submit("what is the capital of norway").unwrap();
    assert_eq!(done.phase, AskPhase::Done);
    assert_eq!(done.answer, "The capital of Norway is Oslo.");
    assert!(done.id > 0 && done.started_at_ms > 0);
    {
        let c = calls.lock().unwrap();
        assert_eq!((c.made, c.warmed), (1, 0));
    }
    // Dismissing a finished answer clears it.
    assert_eq!(finished_answer().as_deref(), Some("The capital of Norway is Oslo."));
    dismiss();
    assert_eq!(snapshot().phase, AskPhase::Idle);
    assert_eq!(finished_answer(), None);

    // A submit while an ask is listening takes that ask over.
    let id = begin_listening().unwrap();
    let done = submit("and of sweden").unwrap();
    assert_eq!((done.id, done.phase), (id, AskPhase::Done));
    let c = calls.lock().unwrap();
    assert_eq!((c.made, c.warmed), (2, 1));
}

#[test]
fn an_empty_question_ends_idle_without_a_call() {
    let calls = calls();
    let _scope = scope(on(), saying(&calls, "Four."));
    let id = begin_listening().unwrap();
    let snap = submit_listening(id, "  \n ", 300).unwrap();
    assert_eq!(snap.phase, AskPhase::Idle);
    assert_eq!((snap.question.as_str(), snap.error.as_str()), ("", ""));
    assert!(!is_busy());
    let c = calls.lock().unwrap();
    assert!(c.sent.is_empty());
    // The backend warmed for it is gone.
    assert_eq!((c.made, c.dropped), (1, 1));
    drop(c);
    assert_eq!(submit("   ").unwrap().phase, AskPhase::Idle);
    assert!(calls.lock().unwrap().sent.is_empty());
}

#[test]
fn a_failed_ask_shows_why_and_has_no_answer_to_hand_on() {
    let calls = calls();
    let _scope = scope(on(), {
        let calls = calls.clone();
        move |_| {
            Box::new(Stub::new(calls.clone(), |input, _, _| {
                if input.text.contains("slow") {
                    Err(AskError::Timeout)
                } else if input.text.contains("signed out") {
                    Err(AskError::Unavailable("Claude Code is installed but not signed in.".into()))
                } else {
                    Err(AskError::Backend("Claude Code: API Error: 529 overloaded".into()))
                }
            }))
        }
    });
    let failed = submit("anything").unwrap();
    assert_eq!(failed.phase, AskPhase::Error);
    assert_eq!(failed.error, "Claude Code: API Error: 529 overloaded");
    assert_eq!(failed.answer, "");
    assert!(!is_busy());
    assert_eq!(finished_answer(), None);

    assert_eq!(submit("a slow one").unwrap().error, "No answer within 60 seconds.");
    assert_eq!(submit("signed out").unwrap().error, "Claude Code is installed but not signed in.");
    // The next ask starts clean.
    assert_eq!(begin_listening().map(|_| snapshot().error), Ok(String::new()));
}

#[test]
fn a_search_shows_on_the_snapshot_while_it_runs() {
    let calls = calls();
    let seen: Arc<Mutex<Vec<AskSnapshot>>> = Arc::new(Mutex::new(Vec::new()));
    let _scope = scope(AskSettings { web_search: true, ..on() }, {
        let (calls, seen) = (calls.clone(), seen.clone());
        move |_| {
            let seen = seen.clone();
            Box::new(Stub::new(calls.clone(), move |_, sink, _| {
                sink(AskEvent::Searching { query: "ferry timetable".into() });
                seen.lock().unwrap().push(snapshot());
                sink(AskEvent::SearchDone);
                seen.lock().unwrap().push(snapshot());
                sink(AskEvent::Text("At 06:40.".into()));
                Ok("At 06:40.".into())
            }))
        }
    });
    let done = submit("when does the first ferry leave").unwrap();
    assert!(done.web_search);
    assert!(!done.searching);
    assert_eq!(done.search_query, "ferry timetable");
    let seen = seen.lock().unwrap();
    assert_eq!((seen[0].phase, seen[0].searching), (AskPhase::Thinking, true));
    assert_eq!(seen[0].search_query, "ferry timetable");
    assert!(!seen[1].searching);
    assert!(calls.lock().unwrap().last_system.contains("You can search the web"));
}

#[test]
fn cancel_while_the_model_works_stops_the_request_at_once() {
    let calls = calls();
    let _scope = scope(on(), waiting(&calls));
    let worker = thread::spawn(|| submit("tell me a long story"));
    wait_until("the answer is streaming", || snapshot().phase == AskPhase::Answering);
    assert!(is_busy());

    assert!(cancel());
    // Without waiting for the backend: the panel reacts at once.
    let cancelled = snapshot();
    assert_eq!(cancelled.phase, AskPhase::Cancelled);
    assert!(!is_busy());
    // What had arrived stays readable, but it is not a finished answer.
    assert_eq!(cancelled.answer, "Partial");
    assert_eq!(finished_answer(), None);

    let ended = worker.join().unwrap().unwrap();
    assert_eq!(ended.phase, AskPhase::Cancelled);
    assert_eq!(ended.error, "");
    assert_eq!(calls.lock().unwrap().dropped, 1, "the backend (and its process) is gone");
    // Nothing left to cancel.
    assert!(!cancel());
    assert_eq!(snapshot().phase, AskPhase::Cancelled);
}

#[test]
fn cancel_while_listening_drops_the_ask_and_its_transcript_is_not_sent() {
    let calls = calls();
    let _scope = scope(on(), saying(&calls, "Four."));
    let id = begin_listening().unwrap();
    assert!(cancel());
    assert_eq!(snapshot().phase, AskPhase::Idle);
    assert!(!is_busy());
    assert_eq!(calls.lock().unwrap().dropped, 1, "the warmed backend is gone");

    // The recording's transcript arrives afterwards.
    assert_eq!(submit_listening(id, "what is two plus two", 900), Err(AskError::Cancelled));
    assert_eq!(snapshot().phase, AskPhase::Idle);
    assert!(calls.lock().unwrap().sent.is_empty());
    // An id that never existed is no better.
    assert_eq!(submit_listening(id + 1000, "what is two plus two", 900), Err(AskError::Cancelled));
}

#[test]
fn a_backend_that_comes_up_after_the_ask_is_gone_is_dropped() {
    let calls = calls();
    let _scope = scope(on(), saying(&calls, "Four."));
    let id = mark_listening().unwrap();
    assert_eq!(snapshot().phase, AskPhase::Listening);
    assert!(cancel());
    // The warm-up thread gets going only now.
    warm_listening(id);
    assert_eq!(calls.lock().unwrap().made, 0);
    assert_eq!(snapshot().phase, AskPhase::Idle);
}

#[test]
fn dismiss_while_answering_goes_idle_and_stays_idle() {
    let calls = calls();
    let _scope = scope(on(), {
        let calls = calls.clone();
        move |_| {
            // Ignores the cancel and finishes its answer anyway.
            Box::new(Stub::new(calls.clone(), |_, sink, cancel| {
                sink(AskEvent::Text("A".into()));
                let deadline = Instant::now() + Duration::from_secs(5);
                while !cancel.is_cancelled() && Instant::now() < deadline {
                    thread::sleep(Duration::from_millis(5));
                }
                sink(AskEvent::Text("B".into()));
                Ok("AB".into())
            }))
        }
    });
    let worker = thread::spawn(|| submit("tell me something"));
    wait_until("the answer is streaming", || snapshot().phase == AskPhase::Answering);
    let id = snapshot().id;

    dismiss();
    let idle = snapshot();
    assert_eq!(idle.phase, AskPhase::Idle);
    assert_eq!((idle.question.as_str(), idle.answer.as_str()), ("", ""));
    assert!(!is_busy());

    // The request ends on its own time and changes nothing.
    let ended = worker.join().unwrap().unwrap();
    assert_eq!(ended.phase, AskPhase::Idle);
    assert_eq!(snapshot().answer, "");
    assert_eq!(finished_answer(), None);
    assert_eq!(snapshot().id, id);
}

#[test]
fn a_new_ask_replaces_the_one_before_and_cancels_it_when_it_still_runs() {
    let calls = calls();
    let _scope = scope(on(), waiting(&calls));
    let worker = thread::spawn(|| submit("the first question"));
    wait_until("the first ask is answering", || snapshot().phase == AskPhase::Answering);
    let first = snapshot().id;

    let second = mark_listening().unwrap();
    assert!(second > first);
    let listening = snapshot();
    assert_eq!((listening.id, listening.phase), (second, AskPhase::Listening));
    assert_eq!((listening.question.as_str(), listening.answer.as_str()), ("", ""));

    // The first request was told to stop, and its end does not show.
    let ended = worker.join().unwrap().unwrap();
    assert_eq!((ended.id, ended.phase), (second, AskPhase::Listening));
    assert_eq!(calls.lock().unwrap().dropped, 1);
}

#[test]
fn a_backend_warmed_for_other_settings_is_not_used() {
    let calls = calls();
    let _scope = scope(on(), saying(&calls, "Fire."));
    let id = begin_listening().unwrap();
    assert_eq!(calls.lock().unwrap().made, 1);
    // The person changes their instructions while talking.
    crate::settings::set_ask_settings_cache(AskSettings { instructions: "Answer in Danish.".into(), ..on() });
    let done = submit_listening(id, "what is two plus two", 800).unwrap();
    assert_eq!(done.phase, AskPhase::Done);
    let c = calls.lock().unwrap();
    assert_eq!((c.made, c.warmed, c.dropped), (2, 1, 2));
    assert!(c.last_system.ends_with("Answer in Danish."), "{}", c.last_system);
}

#[test]
fn phases_have_stable_ids() {
    for phase in [
        AskPhase::Idle,
        AskPhase::Listening,
        AskPhase::Thinking,
        AskPhase::Answering,
        AskPhase::Done,
        AskPhase::Error,
        AskPhase::Cancelled,
    ] {
        assert_eq!(serde_json::to_string(&phase).unwrap(), format!("\"{}\"", phase.as_str()));
        assert_eq!(
            phase.is_busy(),
            matches!(phase, AskPhase::Listening | AskPhase::Thinking | AskPhase::Answering)
        );
    }
    let json = serde_json::to_value(AskSnapshot::default()).unwrap();
    assert_eq!(json["phase"], "idle");
    for key in [
        "id", "question", "answer", "notice", "error", "backend", "model", "effort", "web_search", "searching",
        "search_query", "started_at_ms", "elapsed_ms", "first_text_ms", "usage", "version",
    ] {
        assert!(json.get(key).is_some(), "the snapshot has no {key}");
    }
}

// --- history and diagnostics ----------------------------------------

/// Point history at `dir` (or switch it off) for the test at hand.
fn history_in(dir: Option<&std::path::Path>) {
    crate::settings::set_history_settings_cache(HistorySettings {
        enabled: dir.is_some(),
        retention_days: 0,
        folder: dir.map(|d| d.to_string_lossy().into_owned()),
    });
}

#[test]
fn a_finished_ask_is_kept_in_history_and_a_failed_or_cancelled_one_is_not() {
    // Dictation tests switch the history settings too.
    let _dictation = crate::dictation::TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let calls = calls();
    let _scope = scope(on(), {
        let calls = calls.clone();
        move |_| {
            Box::new(Stub::new(calls.clone(), |input, sink, _| {
                if input.text.contains("fail") {
                    return Err(AskError::Backend("boom".into()));
                }
                if input.text.contains("cancel") {
                    return Err(AskError::Cancelled);
                }
                sink(AskEvent::Text("The capital of Norway is Oslo.".into()));
                Ok("The capital of Norway is Oslo.".into())
            }))
        }
    });
    let dir = tempfile::tempdir().unwrap();
    history_in(Some(dir.path()));

    let id = begin_listening().unwrap();
    let done = submit_listening(id, "what is the capital of norway", 2100).unwrap();
    assert_eq!(done.phase, AskPhase::Done);
    assert_eq!(submit("this will fail").unwrap().phase, AskPhase::Error);
    assert_eq!(submit("please cancel").unwrap().phase, AskPhase::Cancelled);
    assert_eq!(submit("  ").unwrap().phase, AskPhase::Idle);

    let page = crate::history::list(dir.path(), "", 10, 0);
    assert_eq!(page.stored, 1);
    let entry = &page.entries[0];
    assert!(entry.is_ask());
    assert_eq!(entry.text, "what is the capital of norway");
    assert_eq!(entry.answer.as_deref(), Some("The capital of Norway is Oslo."));
    assert_eq!(entry.duration_ms, 2100);
    assert_eq!((entry.app_id.clone(), entry.mode.clone(), entry.original.clone()), (None, None, None));
    // Stored under the time the ask began.
    assert!((entry.at_ms - done.started_at_ms).abs() < 1000);

    // With history off, nothing is written.
    history_in(None);
    let other = tempfile::tempdir().unwrap();
    crate::settings::set_history_settings_cache(HistorySettings {
        enabled: false,
        retention_days: 0,
        folder: Some(other.path().to_string_lossy().into_owned()),
    });
    assert_eq!(submit("what is the capital of norway").unwrap().phase, AskPhase::Done);
    assert_eq!(crate::history::list(other.path(), "", 10, 0).stored, 0);
    crate::settings::set_history_settings_cache(HistorySettings::default());
}

#[test]
fn every_ask_that_reached_the_backend_leaves_a_diagnostics_row_without_its_text() {
    let store = super::test_support::store();
    let calls = calls();
    let _scope = scope(on(), {
        let calls = calls.clone();
        move |_| {
            Box::new(Stub::new(calls.clone(), |input, sink, _| {
                if input.text.contains("slow") {
                    return Err(AskError::Timeout);
                }
                if input.text.contains("fail") {
                    return Err(AskError::Backend("boom".into()));
                }
                if input.text.contains("signed out") {
                    return Err(AskError::Unavailable("not signed in".into()));
                }
                if input.text.contains("cancel") {
                    return Err(AskError::Cancelled);
                }
                sink(AskEvent::Text("Brennholt is in Oslo.".into()));
                Ok("Brennholt is in Oslo.".into())
            }))
        }
    });
    let newest = || runs::recent(store, 1).unwrap().remove(0);

    let id = begin_listening().unwrap();
    let done = submit_listening(id, "where is brennholt", 1500).unwrap();
    let row = newest();
    assert_eq!((row.source.as_str(), row.status.as_str()), ("ask", "answered"));
    assert_eq!((row.backend.as_str(), row.model.as_str(), row.effort.as_str()), ("stub", "sonnet", "low"));
    assert_eq!((row.chars_in, row.chars_out), (18, 21));
    assert_eq!(row.at_ms, done.started_at_ms);
    assert_eq!(row.warm_lead_ms, Some(2500));
    assert_eq!(row.first_text_ms, Some(120));
    assert_eq!((row.tokens_in, row.tokens_cached, row.tokens_out), (Some(900), Some(800), Some(40)));
    assert_eq!(row.cost_usd, Some(0.002));
    assert_eq!((row.detail.as_str(), row.mode.clone(), row.instruction), ("", None, false));
    // Neither the question nor the answer is in the row.
    let json = serde_json::to_string(&row).unwrap().to_lowercase();
    assert!(!json.contains("brennholt") && !json.contains("oslo"), "{json}");

    for (question, status, detail) in [
        ("a slow one", "timeout", "No answer within 60 seconds."),
        ("this will fail", "failed", "boom"),
        ("signed out", "unavailable", "not signed in"),
        ("please cancel", "cancelled", "cancelled"),
    ] {
        submit(question).unwrap();
        let row = newest();
        assert_eq!((row.source.as_str(), row.status.as_str(), row.detail.as_str()), ("ask", status, detail));
        assert_eq!((row.chars_out, row.tokens_in, row.first_text_ms), (0, None, None));
    }

    // An ask that never reached a backend leaves nothing.
    let before = runs::recent(store, 500).unwrap().len();
    submit("   ").unwrap();
    assert_eq!(runs::recent(store, 500).unwrap().len(), before);
}
