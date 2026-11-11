//! What an ask with tools shows of its work (Agent, TASK-99): the
//! steps it took, the confirm it is waiting on, and where it looked
//! for its context.
//!
//! The tools themselves, and the rules for them, are `crate::agent`
//! (cargo feature `agent`). The types here are always compiled with
//! `ask`, because they sit on the snapshot the panel reads
//! ([`super::AskSnapshot`]) and on the events a backend sends
//! ([`super::AskEvent`]); without `agent` an ask simply never has a
//! step.
//!
//! **Every tool call is a step.** A step appears when the model calls
//! a tool and changes when the call ends: done, failed, or denied.
//! What a step holds is short on purpose (one line of the input, one
//! line of the outcome), because it is shown in a small panel and
//! because a tool's result can be long and is not ours to keep.
//!
//! **A confirm is the one moment an ask waits for the person.** A
//! tool that sends, posts, deletes or changes something does not run
//! until they said yes to exactly this call, with its whole input in
//! front of them ([`AskConfirm`]). The backend that asked waits on
//! the gate here ([`Waiting`]); whoever shows the confirm answers it
//! by its request id ([`answer_confirm`], or for the app's one ask
//! [`super::confirm_allow`] and [`super::confirm_deny`]). No answer
//! in time is a deny, and so is a cancel.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use serde::Serialize;

/// What a tool does, as far as its name and its own description of
/// itself tell (`crate::agent::policy` decides, on the careful side).
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum ToolKind {
    /// Looks something up and changes nothing.
    Read,
    /// Prepares a draft the person can still throw away.
    Draft,
    /// Everything else: sends, posts, deletes, changes, and anything
    /// that cannot be told.
    Act,
}

impl ToolKind {
    /// Stable id, same as the serialized form.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Draft => "draft",
            Self::Act => "act",
        }
    }
}

/// Where a step stands.
#[non_exhaustive]
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum StepState {
    /// The tool is running.
    Running,
    /// The tool ran and answered.
    Done,
    /// The tool ran and reported an error.
    Failed,
    /// The tool did not run: it is not allowed in this ask, or the
    /// person said no, or did not answer in time.
    Denied,
    /// The tool waits for the person's yes or no ([`AskConfirm`]).
    Waiting,
}

impl StepState {
    /// Stable id, same as the serialized form.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Denied => "denied",
            Self::Waiting => "waiting",
        }
    }

    /// The step will not change any more.
    pub fn is_final(self) -> bool {
        matches!(self, Self::Done | Self::Failed | Self::Denied)
    }
}

/// Longest one-line summary of a step's input or outcome, in
/// characters.
pub const STEP_LINE_MAX_CHARS: usize = 160;
/// Longest input a confirm shows, in characters. A longer one is cut
/// and says so: the person is then asked to allow something they
/// cannot see in full, and the note tells them.
pub const CONFIRM_INPUT_MAX_CHARS: usize = 4000;

/// One tool call of an ask.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct AskStep {
    /// Tells one call from the next within an ask.
    pub id: String,
    /// Name of the connection the tool belongs to.
    pub connection: String,
    /// The tool's own name (no prefix).
    pub tool: String,
    /// Name to show a person ("Read Thread"); the tool's name when it
    /// has no other.
    pub title: String,
    pub kind: ToolKind,
    /// The input on one line, capped at [`STEP_LINE_MAX_CHARS`].
    pub input_summary: String,
    pub state: StepState,
    /// What came of it, on one line, capped the same way. Empty while
    /// the step runs or waits.
    pub outcome: String,
}

/// A tool call that waits for the person's yes or no.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct AskConfirm {
    /// What an answer names ([`answer_confirm`]).
    pub request_id: String,
    /// The step this is about ([`AskStep::id`]).
    pub step_id: String,
    pub connection: String,
    pub tool: String,
    pub title: String,
    /// The whole input as indented JSON text, so the person sees
    /// exactly what would be sent. Capped at
    /// [`CONFIRM_INPUT_MAX_CHARS`], with a note at the end when it
    /// was cut.
    pub input: String,
    /// The input was longer than the cap and is shown cut.
    pub input_cut: bool,
    /// How long the confirm waits before it counts as a no.
    pub timeout_s: u64,
}

/// How a confirm ended.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ConfirmOutcome {
    /// The person said yes; the tool runs with the input shown.
    Allowed,
    /// The person said no.
    Denied,
    /// Nobody answered in time, which is a no.
    TimedOut,
    /// The ask was cancelled while the confirm waited, which is a no.
    Cancelled,
}

impl ConfirmOutcome {
    /// Stable id, same as the serialized form.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Allowed => "allowed",
            Self::Denied => "denied",
            Self::TimedOut => "timed-out",
            Self::Cancelled => "cancelled",
        }
    }
}

/// Where an ask looked for what the question is about.
#[derive(Serialize, Clone, Debug, PartialEq, Eq, Default)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ContextSource {
    /// Nowhere: the question stood by itself.
    #[default]
    None,
    /// A screenshot of the screen went out with the question.
    Screen,
    /// The app in front has a connection, and the ask was pointed at
    /// that instead of the screen. A screenshot can still have gone
    /// out as well, when the person asked for the screen in so many
    /// words ([`super::AskSnapshot::screen`] says).
    Connection { name: String },
}

impl ContextSource {
    /// One word for a log line or a database column: `none`,
    /// `screen`, or `connection:` and the name.
    pub fn label(&self) -> String {
        match self {
            Self::None => "none".to_string(),
            Self::Screen => "screen".to_string(),
            Self::Connection { name } => format!("connection:{name}"),
        }
    }

    /// The connection, when that is the source.
    pub fn connection(&self) -> Option<&str> {
        match self {
            Self::Connection { name } => Some(name),
            _ => None,
        }
    }
}

/// `text` on one line, capped at `max` characters with an ellipsis.
#[cfg_attr(not(feature = "agent"), allow(dead_code))]
pub(crate) fn one_line(text: &str, max: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        return flat;
    }
    let mut out: String = flat.chars().take(max.saturating_sub(1)).collect();
    out.push('\u{2026}');
    out
}

/// A tool's input for a step: `key: value` pairs on one line, capped.
/// A value that is text is shown as it is, anything else as JSON.
#[cfg_attr(not(feature = "agent"), allow(dead_code))]
pub(crate) fn input_summary(input: &serde_json::Value) -> String {
    let text = match input {
        serde_json::Value::Object(map) => map
            .iter()
            .map(|(key, value)| match value {
                serde_json::Value::String(s) => format!("{key}: {s}"),
                other => format!("{key}: {other}"),
            })
            .collect::<Vec<_>>()
            .join(", "),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    };
    one_line(&text, STEP_LINE_MAX_CHARS)
}

/// A tool's input for a confirm: indented JSON, capped. The flag says
/// that it was cut.
#[cfg_attr(not(feature = "agent"), allow(dead_code))]
pub(crate) fn confirm_input(input: &serde_json::Value) -> (String, bool) {
    let full = serde_json::to_string_pretty(input).unwrap_or_else(|_| input.to_string());
    let total = full.chars().count();
    if total <= CONFIRM_INPUT_MAX_CHARS {
        return (full, false);
    }
    let mut cut: String = full.chars().take(CONFIRM_INPUT_MAX_CHARS).collect();
    cut.push_str(&format!(
        "\n\u{2026} (cut here: {} more characters are not shown)",
        total - CONFIRM_INPUT_MAX_CHARS
    ));
    (cut, true)
}

/// Put `step` into `steps`: in place of the step with the same id, or
/// at the end. A step that reached a final state keeps it: a late
/// "running" for a call that was already denied changes nothing.
pub(crate) fn upsert(steps: &mut Vec<AskStep>, step: AskStep) {
    match steps.iter_mut().find(|s| s.id == step.id) {
        Some(existing) if existing.state.is_final() && !step.state.is_final() => {}
        Some(existing) => *existing = step,
        None => steps.push(step),
    }
}

// --- the gate ------------------------------------------------------

/// The person's answer to a confirm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfirmDecision {
    Allow,
    Deny,
}

/// The confirms that wait right now, by request id, with the answer
/// once there is one.
static GATE: LazyLock<Mutex<HashMap<String, Option<ConfirmDecision>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

fn gate<R>(f: impl FnOnce(&mut HashMap<String, Option<ConfirmDecision>>) -> R) -> R {
    // Every write is one insert or remove, so a poisoned lock still
    // guards a whole map.
    f(&mut GATE.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Answer the confirm `request_id`. Returns whether that confirm was
/// waiting for its first answer: an id nobody waits on, and a second
/// answer to the same confirm, change nothing.
///
/// This is how a caller of [`super::AskPlan::send`] answers the
/// [`super::AskEvent::Confirm`] its sink was given (the CLI's y/n
/// prompt, the eval set). The desktop app's one ask is answered
/// through [`super::confirm_allow`] and [`super::confirm_deny`],
/// which check that the confirm is the one on show first.
pub fn answer_confirm(request_id: &str, decision: ConfirmDecision) -> bool {
    gate(|waiting| match waiting.get_mut(request_id) {
        Some(slot @ None) => {
            *slot = Some(decision);
            true
        }
        _ => false,
    })
}

/// One confirm that waits, held by the backend that asked. Dropping
/// it closes the confirm: an answer that comes later finds nothing.
pub(crate) struct Waiting {
    request_id: String,
}

impl Waiting {
    /// Open the gate for `request_id`.
    pub(crate) fn open(request_id: &str) -> Self {
        gate(|waiting| waiting.insert(request_id.to_string(), None));
        Self { request_id: request_id.to_string() }
    }

    /// The answer, once somebody gave one.
    pub(crate) fn answer(&self) -> Option<ConfirmDecision> {
        gate(|waiting| waiting.get(&self.request_id).copied().flatten())
    }
}

impl Drop for Waiting {
    fn drop(&mut self) {
        gate(|waiting| waiting.remove(&self.request_id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn step(id: &str, state: StepState) -> AskStep {
        AskStep {
            id: id.into(),
            connection: "chat".into(),
            tool: "read_thread".into(),
            title: "Read Thread".into(),
            kind: ToolKind::Read,
            input_summary: String::new(),
            state,
            outcome: String::new(),
        }
    }

    #[test]
    fn ids_are_stable_and_match_the_serialized_form() {
        for (kind, id) in [(ToolKind::Read, "read"), (ToolKind::Draft, "draft"), (ToolKind::Act, "act")] {
            assert_eq!(kind.as_str(), id);
            assert_eq!(serde_json::to_value(kind).unwrap(), id);
        }
        for (state, id) in [
            (StepState::Running, "running"),
            (StepState::Done, "done"),
            (StepState::Failed, "failed"),
            (StepState::Denied, "denied"),
            (StepState::Waiting, "waiting"),
        ] {
            assert_eq!(state.as_str(), id);
            assert_eq!(serde_json::to_value(state).unwrap(), id);
        }
        for (outcome, id) in [
            (ConfirmOutcome::Allowed, "allowed"),
            (ConfirmOutcome::Denied, "denied"),
            (ConfirmOutcome::TimedOut, "timed-out"),
            (ConfirmOutcome::Cancelled, "cancelled"),
        ] {
            assert_eq!(outcome.as_str(), id);
            assert_eq!(serde_json::to_value(outcome).unwrap(), id);
        }
        assert_eq!(serde_json::to_value(ContextSource::None).unwrap(), json!({"kind": "none"}));
        assert_eq!(serde_json::to_value(ContextSource::Screen).unwrap(), json!({"kind": "screen"}));
        let connection = ContextSource::Connection { name: "Team chat".into() };
        assert_eq!(serde_json::to_value(&connection).unwrap(), json!({"kind": "connection", "name": "Team chat"}));
        assert_eq!(connection.label(), "connection:Team chat");
        assert_eq!((ContextSource::None.label(), ContextSource::Screen.label()), ("none".to_string(), "screen".to_string()));
        assert_eq!(ContextSource::default(), ContextSource::None);
    }

    #[test]
    fn an_input_is_one_capped_line_for_a_step_and_whole_json_for_a_confirm() {
        let input = json!({"to": "bo@example.com", "body": "Room\nconfirmed", "urgent": true});
        // One line; the order of the keys is the JSON library's.
        let summary = input_summary(&input);
        for part in ["body: Room confirmed", "to: bo@example.com", "urgent: true"] {
            assert!(summary.contains(part), "{summary}");
        }
        assert!(!summary.contains('\n'));
        assert_eq!(input_summary(&json!(null)), "");
        let long = json!({"body": "word ".repeat(200)});
        let summary = input_summary(&long);
        assert_eq!(summary.chars().count(), STEP_LINE_MAX_CHARS);
        assert!(summary.ends_with('\u{2026}'));

        let (shown, cut) = confirm_input(&input);
        assert!(!cut);
        assert_eq!(serde_json::from_str::<serde_json::Value>(&shown).unwrap(), input);
        assert!(shown.contains("\n  \"to\": \"bo@example.com\""), "{shown}");
        // A very long input is cut, and says by how much.
        let (shown, cut) = confirm_input(&json!({"body": "x".repeat(CONFIRM_INPUT_MAX_CHARS * 2)}));
        assert!(cut);
        assert!(shown.contains("more characters are not shown"), "{}", &shown[shown.len() - 80..]);
        assert!(shown.chars().count() < CONFIRM_INPUT_MAX_CHARS + 100);
    }

    #[test]
    fn a_step_is_replaced_by_its_id_and_a_final_state_stays() {
        let mut steps = Vec::new();
        upsert(&mut steps, step("a", StepState::Running));
        upsert(&mut steps, step("b", StepState::Waiting));
        upsert(&mut steps, step("a", StepState::Done));
        assert_eq!(steps.iter().map(|s| (s.id.as_str(), s.state)).collect::<Vec<_>>(), [
            ("a", StepState::Done),
            ("b", StepState::Waiting)
        ]);
        upsert(&mut steps, step("b", StepState::Denied));
        // A late line about a call that already ended.
        upsert(&mut steps, step("b", StepState::Running));
        assert_eq!(steps[1].state, StepState::Denied);
        // A final state can still be corrected by another final one.
        upsert(&mut steps, step("a", StepState::Failed));
        assert_eq!(steps[0].state, StepState::Failed);
    }

    #[test]
    fn a_confirm_takes_one_answer_and_only_while_it_waits() {
        // Nobody waits on this id.
        assert!(!answer_confirm("steps-test-nobody", ConfirmDecision::Allow));
        let waiting = Waiting::open("steps-test-1");
        assert_eq!(waiting.answer(), None);
        assert!(answer_confirm("steps-test-1", ConfirmDecision::Deny));
        // The first answer stands.
        assert!(!answer_confirm("steps-test-1", ConfirmDecision::Allow));
        assert_eq!(waiting.answer(), Some(ConfirmDecision::Deny));
        drop(waiting);
        // Closed: a late answer finds nothing.
        assert!(!answer_confirm("steps-test-1", ConfirmDecision::Allow));
    }
}
