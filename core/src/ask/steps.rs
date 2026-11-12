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
//!
//! **What is shown is made safe to show** ([`safe_to_show`]). A
//! step's text and a confirm's input come from a tool, a connection
//! or the model: none of it is ours. Characters that would make a
//! terminal or a panel show something other than what is there
//! (control characters, an escape sequence, a right-to-left override,
//! characters that cannot be seen) are replaced by a visible escape.
//! Only what is shown: what a tool is handed after a yes is the
//! input exactly as it came.
//!
//! **A yes has to be meant for this confirm** ([`CONFIRM_ARM_MS`]). A
//! click that was on its way to something else when a confirm came
//! up, or was meant for the confirm before it, must not allow this
//! one: a person's yes counts only once the confirm has been on show
//! for a moment. A no is taken at any time.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

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
/// Longest input a confirm shows, in characters of its indented
/// JSON text. A confirm always shows the whole input, so a call with
/// a longer one is not asked about at all: it is refused, and the
/// model is told that it was too large to be shown.
pub const CONFIRM_INPUT_MAX_CHARS: usize = 20_000;
/// How long a confirm has to be on show before a person's yes to it
/// counts, in milliseconds ([`answer_confirm_as_person`]).
pub const CONFIRM_ARM_MS: u64 = 600;

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
    /// exactly what would be sent: never cut (a call whose input is
    /// longer than [`CONFIRM_INPUT_MAX_CHARS`] is refused without a
    /// confirm). Short fields come first and long text last, and
    /// characters that would not show as what they are are written
    /// as JSON escapes ([`confirm_input`]).
    pub input: String,
    /// How long the confirm waits before it counts as a no.
    pub timeout_s: u64,
    /// How long until a yes to this confirm counts, in milliseconds:
    /// [`CONFIRM_ARM_MS`] when it is raised, counting down to 0 on
    /// the app's snapshot ([`super::snapshot`] reads it off the
    /// clock, like the elapsed time). While it is above 0 a yes is
    /// refused and the confirm stays open; a panel shows its Allow
    /// button as not yet active. A no is taken at any time.
    pub armed_in_ms: u64,
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
    /// The backend stopped while the confirm waited: there is nobody
    /// left to answer, and the tool did not run.
    Gone,
}

impl ConfirmOutcome {
    /// Stable id, same as the serialized form.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Allowed => "allowed",
            Self::Denied => "denied",
            Self::TimedOut => "timed-out",
            Self::Cancelled => "cancelled",
            Self::Gone => "gone",
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

// --- text that is safe to show --------------------------------------

/// `c` would not show as what it is: a control character (C0, DEL,
/// C1; ESC among them), a character that changes the direction text
/// is shown in, one that cannot be seen, or a line or paragraph
/// separator.
fn hides(c: char) -> bool {
    matches!(c,
        '\u{0}'..='\u{1F}' | '\u{7F}'..='\u{9F}'
        // Soft hyphen, and the fillers that show as nothing.
        | '\u{AD}' | '\u{34F}' | '\u{115F}' | '\u{1160}' | '\u{17B4}' | '\u{17B5}' | '\u{180B}'..='\u{180F}'
        | '\u{3164}' | '\u{FFA0}'
        // Direction: the Arabic letter mark, the marks, the
        // embeddings and overrides, the isolates.
        | '\u{61C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'
        // Zero width and other invisible format characters.
        | '\u{200B}'..='\u{200D}' | '\u{2060}'..='\u{2064}' | '\u{206A}'..='\u{206F}' | '\u{FEFF}'
        | '\u{FFF9}'..='\u{FFFB}' | '\u{1D173}'..='\u{1D17A}' | '\u{E0000}'..='\u{E007F}'
        // Line and paragraph separator.
        | '\u{2028}' | '\u{2029}'
    )
}

/// `text` with every character that would not show as what it is
/// replaced by a visible escape (`\u{202E}`, `\u{1B}`): control
/// characters, ESC and DEL, characters that turn the direction of
/// text around, characters that cannot be seen, and line and
/// paragraph separators. With `keep_lines`, a line feed and a tab
/// stay as they are (text that is shown on several lines).
///
/// Everything that is shown of a tool call goes through this: a
/// step's connection, tool, title, input and outcome, a confirm's
/// names, a notice made from what a connection said. The CLI prints
/// names it did not make up through it as well. A tool result that
/// carries an escape sequence then cannot clear a terminal, and a
/// right-to-left override cannot make a confirm read as something
/// other than what would be sent.
pub fn safe_to_show(text: &str, keep_lines: bool) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if hides(c) && !(keep_lines && matches!(c, '\n' | '\t')) {
            out.push_str(&format!("\\u{{{:X}}}", c as u32));
        } else {
            out.push(c);
        }
    }
    out
}

/// `text` on one line, capped at `max` characters with an ellipsis,
/// and safe to show ([`safe_to_show`]).
#[cfg_attr(not(feature = "agent"), allow(dead_code))]
pub(crate) fn one_line(text: &str, max: usize) -> String {
    let flat = safe_to_show(&text.split_whitespace().collect::<Vec<_>>().join(" "), false);
    if flat.chars().count() <= max {
        return flat;
    }
    let mut out: String = flat.chars().take(max.saturating_sub(1)).collect();
    out.push('\u{2026}');
    out
}

/// A tool's input for a step: `key: value` pairs on one line, capped
/// and safe to show. A value that is text is shown as it is, anything
/// else as JSON.
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

/// A string as a JSON string, with every character that would not
/// show as what it is written as a JSON escape (`\u202e`). The text
/// still reads back as the same string.
fn shown_json_string(text: &str) -> String {
    let quoted = serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_string());
    let mut out = String::with_capacity(quoted.len());
    for c in quoted.chars() {
        if hides(c) {
            let mut units = [0u16; 2];
            for unit in c.encode_utf16(&mut units) {
                out.push_str(&format!("\\u{unit:04x}"));
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// How much room a value takes when shown: what a field is ordered
/// by.
fn shown_weight(value: &serde_json::Value) -> usize {
    match value {
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => 0,
        serde_json::Value::String(s) => s.chars().count(),
        // A list or an object is long text for this purpose: at
        // least past every short field.
        other => SHORT_FIELD_MAX_CHARS + 1 + other.to_string().chars().count(),
    }
}

/// A field of a confirm's input up to this many characters is a
/// short one: it is shown before the long ones.
const SHORT_FIELD_MAX_CHARS: usize = 80;

fn write_shown(out: &mut String, value: &serde_json::Value, depth: usize) {
    let pad = |out: &mut String, depth: usize| out.push_str(&"  ".repeat(depth));
    match value {
        serde_json::Value::Object(map) if !map.is_empty() => {
            // Short fields first, in the order they came; then the
            // long ones, shortest first. Who and what is at the top,
            // the long body at the bottom. The sort is stable.
            let mut fields: Vec<(&String, &serde_json::Value)> = map.iter().collect();
            fields.sort_by_key(|(_, value)| {
                let weight = shown_weight(value);
                if weight <= SHORT_FIELD_MAX_CHARS { 0 } else { weight }
            });
            out.push_str("{\n");
            for (at, (key, value)) in fields.iter().enumerate() {
                pad(out, depth + 1);
                out.push_str(&shown_json_string(key));
                out.push_str(": ");
                write_shown(out, value, depth + 1);
                out.push_str(if at + 1 < fields.len() { ",\n" } else { "\n" });
            }
            pad(out, depth);
            out.push('}');
        }
        serde_json::Value::Array(items) if !items.is_empty() => {
            out.push_str("[\n");
            for (at, item) in items.iter().enumerate() {
                pad(out, depth + 1);
                write_shown(out, item, depth + 1);
                out.push_str(if at + 1 < items.len() { ",\n" } else { "\n" });
            }
            pad(out, depth);
            out.push(']');
        }
        serde_json::Value::String(text) => out.push_str(&shown_json_string(text)),
        other => out.push_str(&other.to_string()),
    }
}

/// A tool's input for a confirm: the whole of it as indented JSON
/// text, with the short fields of every object first and its long
/// ones last (so a long `body` cannot push `to` and `subject` out of
/// sight), and with characters that would not show as what they are
/// written as JSON escapes. The text reads back as exactly the
/// input.
///
/// `Err` with the length when the text is longer than
/// [`CONFIRM_INPUT_MAX_CHARS`]: such an input cannot be shown in
/// full, so the call is not asked about.
#[cfg_attr(not(feature = "agent"), allow(dead_code))]
pub(crate) fn confirm_input(input: &serde_json::Value) -> Result<String, usize> {
    let mut shown = String::new();
    write_shown(&mut shown, input, 0);
    let total = shown.chars().count();
    if total > CONFIRM_INPUT_MAX_CHARS { Err(total) } else { Ok(shown) }
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

/// What became of an answer to a confirm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfirmAnswer {
    /// The answer was taken: it is the confirm's answer.
    Taken,
    /// A person's yes that came before the confirm had been on show
    /// for [`CONFIRM_ARM_MS`]: it was not taken, and the confirm is
    /// still open.
    TooSoon,
    /// Nobody waits on that id (any more), or it was answered
    /// already.
    NotWaiting,
}

/// One confirm that waits.
struct Slot {
    raised: Instant,
    answer: Option<ConfirmDecision>,
}

/// The confirms that wait right now, by request id, with the answer
/// once there is one.
static GATE: LazyLock<Mutex<HashMap<String, Slot>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

fn gate<R>(f: impl FnOnce(&mut HashMap<String, Slot>) -> R) -> R {
    // Every write is one insert or remove, so a poisoned lock still
    // guards a whole map.
    f(&mut GATE.lock().unwrap_or_else(|e| e.into_inner()))
}

fn answer(request_id: &str, decision: ConfirmDecision, person: bool) -> ConfirmAnswer {
    gate(|waiting| match waiting.get_mut(request_id) {
        Some(slot) if slot.answer.is_none() => {
            let armed = slot.raised.elapsed() >= Duration::from_millis(CONFIRM_ARM_MS);
            if person && decision == ConfirmDecision::Allow && !armed {
                return ConfirmAnswer::TooSoon;
            }
            slot.answer = Some(decision);
            ConfirmAnswer::Taken
        }
        _ => ConfirmAnswer::NotWaiting,
    })
}

/// Answer the confirm `request_id` for a caller that is not a person
/// at a button: a flag given at the terminal (`openwhisper agent run
/// --confirm allow`), or the eval set. Returns whether that confirm
/// was waiting for its first answer: an id nobody waits on, and a
/// second answer to the same confirm, change nothing.
///
/// A person's own answer goes through [`answer_confirm_as_person`]
/// (the terminal's y/n prompt) or, for the desktop app's one ask,
/// through [`super::confirm_allow`] and [`super::confirm_deny`],
/// which also check that the confirm is the one on show.
pub fn answer_confirm(request_id: &str, decision: ConfirmDecision) -> bool {
    answer(request_id, decision, false) == ConfirmAnswer::Taken
}

/// A person's answer to the confirm `request_id`: a click, or a line
/// typed at a prompt. A no is taken at once. A yes is taken only
/// when the confirm has been on show for [`CONFIRM_ARM_MS`]: one
/// that comes sooner was on its way before the person could have
/// read this confirm (a double click, a click meant for the confirm
/// before), so it is refused, **not** counted as a no, and the
/// confirm stays open for a real answer.
pub fn answer_confirm_as_person(request_id: &str, decision: ConfirmDecision) -> ConfirmAnswer {
    answer(request_id, decision, true)
}

/// How long until a person's yes to the confirm `request_id` counts,
/// in milliseconds; 0 when it counts now. `None` when nobody waits
/// on that id.
pub fn confirm_armed_in_ms(request_id: &str) -> Option<u64> {
    gate(|waiting| {
        waiting.get(request_id).map(|slot| CONFIRM_ARM_MS.saturating_sub(slot.raised.elapsed().as_millis() as u64))
    })
}

/// One confirm that waits, held by the backend that asked. Dropping
/// it closes the confirm: an answer that comes later finds nothing.
pub(crate) struct Waiting {
    request_id: String,
}

impl Waiting {
    /// Open the gate for `request_id`. The confirm counts as raised
    /// from now.
    pub(crate) fn open(request_id: &str) -> Self {
        gate(|waiting| waiting.insert(request_id.to_string(), Slot { raised: Instant::now(), answer: None }));
        Self { request_id: request_id.to_string() }
    }

    /// The answer, once somebody gave one.
    pub(crate) fn answer(&self) -> Option<ConfirmDecision> {
        gate(|waiting| waiting.get(&self.request_id).and_then(|slot| slot.answer))
    }

    /// Make the confirm count as raised `ago` earlier. For tests
    /// that answer as a person without waiting.
    #[cfg(test)]
    pub(crate) fn backdate(&self, ago: Duration) {
        gate(|waiting| {
            if let Some(slot) = waiting.get_mut(&self.request_id) {
                slot.raised = Instant::now().checked_sub(ago).unwrap_or(slot.raised);
            }
        });
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
            (ConfirmOutcome::Gone, "gone"),
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

        let shown = confirm_input(&input).unwrap();
        assert_eq!(serde_json::from_str::<serde_json::Value>(&shown).unwrap(), input);
        assert!(shown.contains("\n  \"to\": \"bo@example.com\""), "{shown}");
    }

    #[test]
    fn a_confirm_shows_the_whole_input_with_who_and_what_on_top_or_is_not_raised_at_all() {
        // A long body, which sorts before `subject` and `to` by its
        // name, and would have pushed both out of a cut text.
        let body = "Dear Bo,\n".to_string() + &"The room is confirmed for Thursday. ".repeat(200);
        let input = json!({
            "body": body,
            "cc": ["anna@example.com"],
            "subject": "Room",
            "to": "all@example.com",
            "urgent": true,
            "options": { "footer": "x".repeat(300), "sign": false },
        });
        let shown = confirm_input(&input).unwrap();
        // Nothing is cut, and it reads back as exactly the input.
        assert_eq!(serde_json::from_str::<serde_json::Value>(&shown).unwrap(), input);
        assert!(!shown.contains("not shown") && !shown.contains('\u{2026}'));
        // The short fields first, in the order they came; then the
        // long ones, the longest last.
        let at = |key: &str| shown.find(&format!("\n  \"{key}\":")).unwrap_or_else(|| panic!("{key} in {shown}"));
        assert!(at("subject") < at("to") && at("to") < at("urgent"), "{shown}");
        assert!(at("urgent") < at("cc") && at("cc") < at("options") && at("options") < at("body"), "{shown}");
        assert!(at("to") < 60, "the recipient is in the first lines: {}", &shown[..120]);
        // Inside an object as well.
        assert!(shown.find("\"sign\"").unwrap() < shown.find("\"footer\"").unwrap());
        // The same input always shows the same way.
        assert_eq!(confirm_input(&input).unwrap(), shown);

        // Up to the cap it is shown, whole.
        let fits = json!({ "body": "x".repeat(CONFIRM_INPUT_MAX_CHARS - 100), "to": "bo@example.com" });
        let shown = confirm_input(&fits).unwrap();
        assert!(shown.chars().count() <= CONFIRM_INPUT_MAX_CHARS);
        assert_eq!(serde_json::from_str::<serde_json::Value>(&shown).unwrap(), fits);
        // Past it there is no text to show: the call is not asked
        // about, and the length says why.
        let huge = json!({ "body": "x".repeat(CONFIRM_INPUT_MAX_CHARS), "to": "all@example.com" });
        let total = confirm_input(&huge).unwrap_err();
        assert!(total > CONFIRM_INPUT_MAX_CHARS);
        // Scalars and empties show as JSON has them.
        assert_eq!(confirm_input(&json!({})).unwrap(), "{}");
        assert_eq!(confirm_input(&json!({ "n": 3, "none": null, "list": [] })).unwrap(), "{\n  \"n\": 3,\n  \"none\": null,\n  \"list\": []\n}");
    }

    #[test]
    fn text_that_would_not_show_as_what_it_is_is_shown_as_escapes() {
        // An escape sequence that clears a terminal, a bell, a
        // carriage return that overwrites the line.
        assert_eq!(safe_to_show("ok\u{1b}[2J\u{1b}[H\u{7}", false), "ok\\u{1B}[2J\\u{1B}[H\\u{7}");
        assert_eq!(safe_to_show("sent\rnot sent", false), "sent\\u{D}not sent");
        // A right-to-left override: "to: all@example.com" made to
        // read as something else.
        assert_eq!(safe_to_show("to: \u{202e}moc.elpmaxe@lla", false), "to: \\u{202E}moc.elpmaxe@lla");
        for (hidden, shown) in [
            ('\u{0}', "\\u{0}"),
            ('\u{7f}', "\\u{7F}"),
            ('\u{85}', "\\u{85}"),
            ('\u{9b}', "\\u{9B}"),
            ('\u{ad}', "\\u{AD}"),
            ('\u{61c}', "\\u{61C}"),
            ('\u{200b}', "\\u{200B}"),
            ('\u{200d}', "\\u{200D}"),
            ('\u{200e}', "\\u{200E}"),
            ('\u{200f}', "\\u{200F}"),
            ('\u{202a}', "\\u{202A}"),
            ('\u{2028}', "\\u{2028}"),
            ('\u{2029}', "\\u{2029}"),
            ('\u{2060}', "\\u{2060}"),
            ('\u{2066}', "\\u{2066}"),
            ('\u{2069}', "\\u{2069}"),
            ('\u{feff}', "\\u{FEFF}"),
            ('\u{e0041}', "\\u{E0041}"),
        ] {
            assert_eq!(safe_to_show(&format!("a{hidden}b"), true), format!("a{shown}b"), "{shown}");
        }
        // Lines and tabs stay where text is shown on several lines,
        // and only there.
        assert_eq!(safe_to_show("one\n\ttwo", true), "one\n\ttwo");
        assert_eq!(safe_to_show("one\n\ttwo", false), "one\\u{A}\\u{9}two");
        // Ordinary text, in any script, is left alone.
        let plain = "Anna: caf\u{e9}, \u{65e5}\u{672c}\u{8a9e}, \u{5e9}\u{5dc}\u{5d5}\u{5dd}, \u{1F600} 100%";
        assert_eq!(safe_to_show(plain, false), plain);

        // One line of a step: flat, capped, safe.
        assert_eq!(one_line("ok\u{1b}[2J  done\n\u{202e}now", 80), "ok\\u{1B}[2J done \\u{202E}now");
        assert_eq!(input_summary(&json!({ "to": "bo\u{202e}@example.com" })), "to: bo\\u{202E}@example.com");

        // A confirm's input: keys and values, as JSON escapes, so the
        // text still reads back as exactly what would be sent.
        let input = json!({ "to": "\u{202e}moc.elpmaxe@lla", "bo\u{200b}dy": "hi\u{1b}[2J\u{feff}\u{e0041}" });
        let shown = confirm_input(&input).unwrap();
        assert!(shown.is_ascii(), "{shown}");
        assert!(shown.contains("\"\\u202emoc.elpmaxe@lla\""), "{shown}");
        assert!(shown.contains("\"bo\\u200bdy\": \"hi\\u001b[2J\\ufeff\\udb40\\udc41\""), "{shown}");
        assert_eq!(serde_json::from_str::<serde_json::Value>(&shown).unwrap(), input);
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
        assert_eq!(confirm_armed_in_ms("steps-test-nobody"), None);
        let waiting = Waiting::open("steps-test-1");
        assert_eq!(waiting.answer(), None);
        assert!(answer_confirm("steps-test-1", ConfirmDecision::Deny));
        // The first answer stands.
        assert!(!answer_confirm("steps-test-1", ConfirmDecision::Allow));
        assert_eq!(waiting.answer(), Some(ConfirmDecision::Deny));
        drop(waiting);
        // Closed: a late answer finds nothing.
        assert!(!answer_confirm("steps-test-1", ConfirmDecision::Allow));
        assert_eq!(answer_confirm_as_person("steps-test-1", ConfirmDecision::Deny), ConfirmAnswer::NotWaiting);
    }

    #[test]
    fn a_persons_yes_counts_only_once_the_confirm_has_been_on_show_for_a_moment() {
        let waiting = Waiting::open("steps-test-arm");
        let left = confirm_armed_in_ms("steps-test-arm").unwrap();
        assert!(left > CONFIRM_ARM_MS - 200 && left <= CONFIRM_ARM_MS, "{left}");
        // A click that was already on its way: refused, not a no,
        // and the confirm is still open.
        assert_eq!(answer_confirm_as_person("steps-test-arm", ConfirmDecision::Allow), ConfirmAnswer::TooSoon);
        assert_eq!(answer_confirm_as_person("steps-test-arm", ConfirmDecision::Allow), ConfirmAnswer::TooSoon);
        assert_eq!(waiting.answer(), None);
        // On show long enough: the same click counts.
        waiting.backdate(Duration::from_millis(CONFIRM_ARM_MS));
        assert_eq!(confirm_armed_in_ms("steps-test-arm"), Some(0));
        assert_eq!(answer_confirm_as_person("steps-test-arm", ConfirmDecision::Allow), ConfirmAnswer::Taken);
        assert_eq!(waiting.answer(), Some(ConfirmDecision::Allow));
        assert_eq!(answer_confirm_as_person("steps-test-arm", ConfirmDecision::Deny), ConfirmAnswer::NotWaiting);
        drop(waiting);

        // A no is taken at any time.
        let waiting = Waiting::open("steps-test-arm-no");
        assert_eq!(answer_confirm_as_person("steps-test-arm-no", ConfirmDecision::Deny), ConfirmAnswer::Taken);
        assert_eq!(waiting.answer(), Some(ConfirmDecision::Deny));
        drop(waiting);

        // The real clock, once: too soon, then after the wait not.
        let waiting = Waiting::open("steps-test-arm-clock");
        assert_eq!(answer_confirm_as_person("steps-test-arm-clock", ConfirmDecision::Allow), ConfirmAnswer::TooSoon);
        std::thread::sleep(Duration::from_millis(CONFIRM_ARM_MS + 30));
        assert_eq!(answer_confirm_as_person("steps-test-arm-clock", ConfirmDecision::Allow), ConfirmAnswer::Taken);
        drop(waiting);

        // A flag at a terminal and the eval set are not a click.
        let waiting = Waiting::open("steps-test-arm-flag");
        assert!(answer_confirm("steps-test-arm-flag", ConfirmDecision::Allow));
        drop(waiting);
    }
}
