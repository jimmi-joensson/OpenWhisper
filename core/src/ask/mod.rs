//! Ask: a spoken question, answered by a language model and shown in
//! a panel instead of being typed (TASK-97).
//!
//! ```text
//! ask key --> record --> transcribe (local) --> rules + dictionary --> ASK --> panel
//!   |                                                                  ^
//!   +-- begin_listening(): backend starts up meanwhile ----------------+
//! ```
//!
//! The same microphone, recorder and local transcription as a
//! dictation ([`crate::dictation`]); what differs is where the text
//! goes. A dictation is typed where the cursor is. An ask is sent to
//! the model as a question, and the answer only ever exists on the
//! snapshot here ([`AskSnapshot`]) until the person copies or inserts
//! it themselves. Nothing in this module calls the injector.
//!
//! Three rules shape it:
//!
//! - **Opt-in.** An ask sends the question off the machine, so it has
//!   its own switch ([`crate::settings::AskSettings::enabled`], off by
//!   default). The backend is the one Refine is set up with: the
//!   user's Claude Code CLI or an OpenAI-compatible endpoint.
//! - **An ask never changes anything.** The model gets no tools at
//!   all, so it can answer and nothing else. The one exception is the
//!   built-in web search, and only while the user has switched that
//!   on. The Claude CLI is always started with `--permission-mode
//!   default`, so a machine whose own default is a looser mode cannot
//!   widen this. A backend that sees the model reach for any other
//!   tool stops and reports an error.
//! - **The answer is never typed by itself.** See above.
//!
//! An ask also tells the model a few facts about the computer it was
//! asked on ([`context`]: the date and time, the machine, the app in
//! front, the language and region, and where the person says they
//! are), each group behind its own switch, and its answers hold no
//! em dash and no en dash while that is switched on
//! ([`crate::settings::RefineSettings::no_long_dashes`], [`crate::dashes`]).
//!
//! This is a separate pipeline from [`crate::refine::Refiner`], which
//! is text in, text out within a few seconds. An answer is long, so
//! it streams ([`AskEvent`]); it can be cancelled ([`CancelFlag`]);
//! and a backend is a conversation that can take more than one turn
//! ([`AskBackend`]), which is how the screen is sent after the
//! question when the model asks for it.
//!
//! **The screen (Vision, TASK-98).** An ask can carry a screenshot of
//! the display the person is working on. Whether it does is decided
//! here, on the machine, from the Vision settings and the question's
//! own words ([`crate::settings::VisionSettings`], [`VisionChoice`]):
//!
//! - no capture: the ask is what it always was;
//! - capture (a phrase matched, or every ask carries the screen): the
//!   screenshot is taken and goes out with the question;
//! - smart, no phrase: the question goes out as text, with one extra
//!   rule in the prompt: reply [`NEED_SCREEN`] and nothing else when
//!   the screen is needed. Only then is the screen captured and sent
//!   as a second turn of the same conversation. That marker never
//!   reaches the panel: text is held back until it is clear that the
//!   reply is not it. A model does not always reply with the bare
//!   word, so the marker is also recognised dressed up, and after one
//!   short sentence of preamble ([`hint`] has the rules); for that,
//!   the first line of an answer is held back in this mode until it
//!   is clear that it is not such a preamble.
//!
//! When a screenshot was wanted and could not be taken (an app on the
//! never-capture list is in front, no permission, no display), the
//! ask goes on as text, the snapshot says why ([`ScreenState`],
//! [`AskSnapshot::screen_notice`]), and the model is told why, so the
//! answer says that the screen was not seen instead of guessing.
//!
//! The image exists from the capture until the backend call that
//! sends it returns, in memory only. This module is the only code
//! that can ask for one: [`crate::vision::capture_for_ask`] takes an
//! [`AskCaptureToken`], which only this module can make. The
//! dictation pipeline ends in [`submit_listening`] with a transcript
//! and has no other way in.
//!
//! **Tools (Agent, TASK-99).** An ask can be given tools that reach
//! into the person's own apps (`crate::agent`, cargo feature
//! `agent`, off by default in the settings). It is then still an
//! ask: the same question, the same panel, the same answer that is
//! never typed by itself. What is added shows on the snapshot:
//!
//! - every tool call is a step ([`AskStep`], [`AskSnapshot::steps`]);
//! - a tool that sends, posts, deletes or changes something does not
//!   run until the person said yes to exactly that call. While it
//!   waits, the ask is `confirming` ([`AskPhase::Confirming`],
//!   [`AskSnapshot::confirm`]); [`confirm_allow`] and
//!   [`confirm_deny`] answer it, no answer in time is a no, and so
//!   is a cancel;
//! - where the ask looked for what the question is about
//!   ([`ContextSource`]): when the app in front has a connection the
//!   ask can use, it is pointed at that and takes no screenshot,
//!   unless the person asked for the screen in so many words.
//!
//! The rule "an ask never changes anything" holds for every ask
//! without tools exactly as before, with the same arguments to the
//! CLI. An ask with tools can change something only through a tool
//! the person ticked, and for anything but reading and drafting only
//! after their yes.
//!
//! Entry points:
//!
//! - one ask at a time, process-wide, for the desktop app:
//!   [`begin_listening`] when the ask recording starts, [`submit`]
//!   with the transcript (or typed text), [`cancel`], [`dismiss`],
//!   and [`snapshot`] for the panel. The dictation pipeline uses the
//!   split forms [`mark_listening`], [`warm_listening`],
//!   [`submit_listening`] and [`fail_listening`], which tie the steps
//!   of one recording together by id. Whoever stops or closes an ask
//!   it saw a moment ago says which one ([`cancel_if`],
//!   [`dismiss_if`]), so a new ask that started meanwhile is left
//!   alone; the plain [`cancel`] and [`dismiss`] are for "whatever is
//!   there now" (the cancel key). A confirm is answered with
//!   [`confirm_allow`] or [`confirm_deny`], which name the ask and
//!   the confirm the same way.
//! - the plain function for everything else (the CLI, the eval set):
//!   [`answer`], or [`AskPlan`] when the backend should be warmed
//!   first ([`AskPlan::send_and_record`] to keep what the app keeps
//!   of an ask). No global state. Such an ask does not look at the
//!   screen unless its caller says so ([`AskPlan::with_vision`]).

use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::refine::history::{self as runs, RefineRun};
use crate::refine::{BackendStatus, RefineError};
use crate::settings::{
    ASK_TIMEOUT_MAX_MS, AskSettings, ClaudeCliSettings, OpenAiSettings, RefineBackendKind, RefineEffort,
    RefineSettings,
};

pub mod claude_cli;
pub mod context;
pub mod eval;
pub mod hint;
pub mod openai_compat;
pub mod prompt;
mod screen;
pub(crate) mod steps;
pub(crate) mod wipe;

pub use crate::refine::CallUsage;
pub use prompt::ScreenPrompt;
pub use screen::{AskCaptureToken, NEED_SCREEN, ScreenState, VisionChoice};
#[cfg(feature = "agent")]
pub(crate) use screen::StandIn;
pub use steps::{
    AskConfirm, AskStep, CONFIRM_ARM_MS, CONFIRM_INPUT_MAX_CHARS, ConfirmAnswer, ConfirmDecision, ConfirmOutcome,
    ContextSource, STEP_LINE_MAX_CHARS, StepState, ToolKind, answer_confirm, answer_confirm_as_person, safe_to_show,
};

use crate::settings::VisionSettings;
use screen::{Shot, Step};

// --- what goes in and what comes out -------------------------------

/// An image sent along with a question: the screenshot of an ask
/// that includes the screen (TASK-98). It lives until the backend
/// call that sends it returns.
///
/// **What is wiped, and what is not.** The bytes in `data` are
/// overwritten with zeros when the image is dropped, and so is every
/// copy this crate makes of them on the way out: a clone of the image,
/// the base64 text of the Claude CLI's input line
/// ([`claude_cli::user_line`]) once it is written to the process, and
/// the `data:` URL of an OpenAI-compatible request
/// ([`openai_compat`]) once the request has returned, where the image
/// is also taken out of the kept conversation. What this crate
/// cannot reach is not wiped: the copy inside the HTTP client and the
/// TLS layer while a request is on its way, the pipe and the memory
/// of the Claude CLI process the line was written to, and whatever
/// the allocator did with a buffer that grew. Those end with the
/// request and with that process.
#[derive(Clone, PartialEq, Eq)]
pub struct AskImage {
    /// MIME type, e.g. `image/png`.
    pub media_type: String,
    /// The file's bytes, not encoded.
    pub data: Vec<u8>,
}

/// Never prints the bytes: a screenshot does not belong in a log.
impl fmt::Debug for AskImage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AskImage({}, {} bytes)", self.media_type, self.data.len())
    }
}

impl Drop for AskImage {
    fn drop(&mut self) {
        wipe::bytes(&mut self.data);
    }
}

/// One turn sent to the model.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AskInput {
    pub text: String,
    pub images: Vec<AskImage>,
}

impl AskInput {
    /// A turn that is only text.
    pub fn text(text: impl Into<String>) -> Self {
        Self { text: text.into(), images: Vec::new() }
    }
}

/// What happens while an answer is on its way.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AskEvent {
    /// The next piece of the answer. Pieces in order make the answer.
    Text(String),
    /// The model is searching the web for this.
    Searching { query: String },
    /// That search is back; the model carries on.
    SearchDone,
    /// What is happening with the screen: it is being captured, a
    /// screenshot goes out with the question, or there is none and
    /// `notice` says why (one sentence for the person). Never carries
    /// the image.
    Screen { state: ScreenState, notice: String },
    /// The answer comes with a suggestion for the person
    /// ([`AskReply::suggestion`]). Sent once, as soon as it is known,
    /// which is before the text of the answer.
    Suggestion(AskSuggestion),
    /// A tool call started, or changed (an ask with tools). It takes
    /// the place of the step with the same [`AskStep::id`], or is a
    /// new one.
    Step(AskStep),
    /// A tool call waits for the person's yes or no. Whoever shows
    /// it answers with [`answer_confirm`] (the app's own ask:
    /// [`confirm_allow`], [`confirm_deny`]). An answer given from
    /// inside the sink is found at once.
    Confirm(AskConfirm),
    /// That confirm is over, and how.
    ConfirmClosed { request_id: String, outcome: ConfirmOutcome },
    /// Where the ask looks for what the question is about, when that
    /// is a connection. Sent once, before anything goes to the
    /// model.
    Context(ContextSource),
}

/// Something the person could do to get a better answer next time.
/// The model decides when it applies (it marks its answer, see
/// [`hint`]); the app says it in its own words and never shows
/// anything the model wrote for it.
#[non_exhaustive]
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AskSuggestion {
    /// The question needed current information from the web, and web
    /// search is switched off
    /// ([`crate::settings::AskSettings::web_search`]).
    WebSearch,
}

impl AskSuggestion {
    /// Stable id, same as the serialized form.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WebSearch => "web-search",
        }
    }
}

/// A finished answer.
#[derive(Serialize, Clone, Debug, PartialEq, Default)]
pub struct AskReply {
    /// The whole answer, trimmed.
    pub text: String,
    /// From sending the question to the first piece of text. `None`
    /// when the backend did not stream.
    pub first_text_ms: Option<u64>,
    /// From sending the question to the end of the answer.
    pub elapsed_ms: u64,
    /// Tokens, list-price cost and plan windows, as far as the
    /// backend reports them.
    pub usage: Option<CallUsage>,
    /// When the backend had been started ahead of the question (at
    /// record start): how many ms ahead. `None` = it started cold, or
    /// this was a later turn of the same conversation.
    pub warm_lead_ms: Option<u64>,
    /// Something the person should know about how the answer was
    /// made ("web search is not available with this backend"). Empty
    /// most of the time.
    pub notice: String,
    /// What became of the screen in this ask: `included` when a
    /// screenshot went out with the question, `blocked` or
    /// `unavailable` when one was wanted and there is none, `none`
    /// when the ask never involved the screen.
    pub screen: ScreenState,
    /// Why there is no screenshot, for the person. Empty unless
    /// `screen` is `blocked` or `unavailable`.
    pub screen_notice: String,
    /// See [`AskSuggestion`]. `None` most of the time.
    pub suggestion: Option<AskSuggestion>,
    /// The tool calls of this ask, in the order they were made, each
    /// as it ended. Empty for an ask without tools.
    pub steps: Vec<AskStep>,
    /// Where the ask looked for what the question is about.
    pub context_source: ContextSource,
}

/// Why there is no answer.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AskError {
    /// The backend cannot run at all: Ask is switched off, the CLI is
    /// not installed or not signed in, the endpoint is not set up.
    /// The message says what to do.
    Unavailable(String),
    /// No answer within the timeout.
    Timeout,
    /// The person cancelled.
    Cancelled,
    /// The backend ran and failed.
    Backend(String),
}

impl fmt::Display for AskError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(m) | Self::Backend(m) => f.write_str(m),
            Self::Timeout => f.write_str("no answer in time"),
            Self::Cancelled => f.write_str("cancelled"),
        }
    }
}

impl std::error::Error for AskError {}

/// Both features talk to the same backends, so a missing or
/// signed-out CLI reads the same in Ask as in Refine.
impl From<RefineError> for AskError {
    fn from(e: RefineError) -> Self {
        match e {
            RefineError::Unavailable(m) => Self::Unavailable(m),
            RefineError::Timeout => Self::Timeout,
            RefineError::Backend(m) => Self::Backend(m),
        }
    }
}

/// Shared between whoever may cancel an ask and the backend that runs
/// it. Backends check it at least every [`POLL`], also while nothing
/// arrives from the model.
#[derive(Clone, Debug, Default)]
pub struct CancelFlag(Arc<AtomicBool>);

impl CancelFlag {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ask the backend to stop. Cannot be undone.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// How often a backend that is waiting for the model looks at the
/// cancel flag and the clock.
pub(crate) const POLL: Duration = Duration::from_millis(25);

/// What a conversation is started with. Fixed for its lifetime: a
/// backend given another setup starts over.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AskSetup {
    /// The system prompt ([`prompt::system_prompt`]).
    pub system: String,
    pub model: String,
    /// `None` sends no effort (OpenAI-compatible only; the Claude CLI
    /// then gets low).
    pub effort: Option<RefineEffort>,
    /// The model may use the built-in web search. No other tool
    /// exists for an ask, whatever this says.
    pub web_search: bool,
}

/// An Ask backend: a conversation with a model.
///
/// Dropping the backend ends the conversation and releases whatever
/// it started (kills a child process, closes a connection).
pub trait AskBackend: Send {
    /// Stable id for logs and the UI, e.g. `"claude-cli"`.
    fn id(&self) -> &'static str;

    /// Start anything that can overlap the recording, so the first
    /// [`AskBackend::send`] is fast. Must not send anything to the
    /// model (there is no question yet).
    fn warm(&mut self, setup: &AskSetup);

    /// Send one turn and stream the answer into `sink`. `input.text`
    /// goes out as given. Blocks until the answer is complete.
    ///
    /// Returns [`AskError::Cancelled`] within a few [`POLL`]s of
    /// `cancel` being set, and [`AskError::Timeout`] when the backend
    /// takes longer than `timeout`: for the whole turn (the Claude
    /// CLI) or without sending anything (a streaming endpoint, which
    /// may be a slow local model that is making steady progress).
    /// After an error the conversation is over. After an answer,
    /// `send` may be called again with the same setup for a further
    /// turn in the same conversation.
    fn send(
        &mut self,
        setup: &AskSetup,
        input: &AskInput,
        sink: &mut dyn FnMut(AskEvent),
        cancel: &CancelFlag,
        timeout: Duration,
    ) -> Result<AskReply, AskError>;
}

/// The text and timings of one turn as it streams in. Backends feed
/// it; it feeds the sink.
pub(crate) struct Turn<'a> {
    sink: &'a mut dyn FnMut(AskEvent),
    started: Instant,
    first_text: Option<Duration>,
    text: String,
    /// A new message began after text was already shown (the model
    /// spoke, searched, and speaks again): its text starts a new
    /// paragraph.
    gap: bool,
}

impl<'a> Turn<'a> {
    pub(crate) fn new(sink: &'a mut dyn FnMut(AskEvent), started: Instant) -> Self {
        Self { sink, started, first_text: None, text: String::new(), gap: false }
    }

    /// The next piece of the answer.
    pub(crate) fn text(&mut self, delta: &str) {
        if delta.is_empty() {
            return;
        }
        let mut piece = String::new();
        if self.gap {
            self.gap = false;
            if !self.text.ends_with('\n') {
                piece.push_str("\n\n");
            }
        }
        piece.push_str(delta);
        self.first_text.get_or_insert_with(|| self.started.elapsed());
        self.text.push_str(&piece);
        (self.sink)(AskEvent::Text(piece));
    }

    /// The model started another message in the same turn.
    pub(crate) fn new_message(&mut self) {
        if !self.text.is_empty() {
            self.gap = true;
        }
    }

    pub(crate) fn event(&mut self, event: AskEvent) {
        (self.sink)(event);
    }

    pub(crate) fn has_text(&self) -> bool {
        !self.text.trim().is_empty()
    }

    pub(crate) fn finish(self, usage: Option<CallUsage>, warm_lead: Option<Duration>, notice: &str) -> AskReply {
        AskReply {
            text: prompt::clean_answer(&self.text),
            first_text_ms: self.first_text.map(|d| d.as_millis() as u64),
            elapsed_ms: self.started.elapsed().as_millis() as u64,
            usage,
            warm_lead_ms: warm_lead.map(|d| d.as_millis() as u64),
            notice: notice.to_string(),
            screen: ScreenState::None,
            screen_notice: String::new(),
            suggestion: None,
            steps: Vec::new(),
            context_source: ContextSource::None,
        }
    }
}

// --- one ask, worked out -------------------------------------------

/// Said in the reply when web search was asked for and the backend
/// has none.
pub const WEB_SEARCH_UNAVAILABLE: &str =
    "Web search is only available with the Claude backend. This answer comes from the model alone.";

const ASK_OFF: &str = "Ask is switched off. Switch it on in the settings to ask questions.";
const NO_QUESTION: &str = "There was no question to answer.";
const NO_ANSWER: &str = "The model returned no answer.";
const STOPPED: &str = "The ask stopped on an internal error. Ask again.";

/// The answer when the model wrote the web search marker and nothing
/// else.
pub const NEEDS_THE_WEB: &str = "This needs current information from the web.";

/// Everything one ask is made with, worked out from the two settings
/// blocks: which backend, the conversation setup, the timeout. The
/// app and the CLI go through the same type, so `openwhisper ask run`
/// does exactly what the ask key does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AskPlan {
    /// The backend Refine is set up with.
    pub backend: RefineBackendKind,
    pub setup: AskSetup,
    /// [`AskSettings::timeout_ms`], doubled while web search is in
    /// use: a search alone takes 15 to 20 seconds.
    pub timeout: Duration,
    /// See [`AskReply::notice`].
    pub notice: String,
    /// Web search is off and the backend has one to switch on (the
    /// Claude backend). The prompt then asks the model to mark an
    /// answer that needed the web ([`prompt::WebAccess::SwitchedOff`]);
    /// [`AskPlan::send`] takes the mark off the answer and turns it
    /// into [`AskReply::suggestion`].
    pub web_hint: bool,
    /// The facts about the computer the prompt carries, as rendered
    /// ([`context::DeviceContext::render`]). Empty = none. It is part
    /// of [`AskSetup::system`]; kept here as well so the ask that was
    /// warmed with it can be sent with the same one.
    pub context: String,
    /// Em dashes and en dashes are taken out of the answer, as it
    /// streams and at the end
    /// ([`crate::settings::RefineSettings::no_long_dashes`]).
    pub no_long_dashes: bool,
    claude_cli: ClaudeCliSettings,
    openai: OpenAiSettings,
    /// What the system prompt is made of, to make it again once it is
    /// known whether a screenshot goes along.
    instructions: String,
    terms: Vec<String>,
    today: String,
    /// Whether and when this ask includes the screen.
    vision: screen::AskVision,
    /// The model may be told which app is in front: the person has
    /// that fact switched on ([`crate::settings::ContextSettings::front_app`]).
    /// When it is off, the reason for a missing screenshot does not
    /// name the protected app that is in front either.
    names_front_app: bool,
    /// The tools of this ask, when it has any (Agent, TASK-99;
    /// [`AskPlan::with_agent`]). `None` = an ask without tools: the
    /// ask from before Agent, with the same prompt and the same
    /// arguments to the CLI.
    #[cfg(feature = "agent")]
    agent: Option<crate::agent::AgentRun>,
    /// What the system prompt says about those tools
    /// ([`prompt::Extras::tools`]). Empty for an ask without tools.
    tools_prompt: String,
    /// The connection this ask is pointed at, because the app in
    /// front has one. Then no screenshot is taken unless the person
    /// asked for the screen in so many words.
    routed_to: Option<String>,
    /// An image that stands in for the screen ([`screen::StandIn`]).
    stand_in: Option<screen::StandIn>,
}

/// What an ask did besides answering, known also when there is no
/// answer: whether a screenshot was taken, which tools were called,
/// where it looked for its context. It is what the diagnostics row
/// records ([`Outcome::record`]), so an ask that took a screenshot or
/// called a tool and then failed or was cancelled still says so.
#[derive(Clone, Debug, Default)]
struct Trace {
    screen: ScreenState,
    steps: Vec<AskStep>,
    context_source: ContextSource,
    /// The answer as far as it reached the sink: the whole of it
    /// when the ask finished, the part that had arrived when it did
    /// not.
    text: String,
}

/// The markers one turn of an ask looks for in the reply.
#[derive(Clone, Copy, Debug)]
struct Watch {
    /// [`NEED_SCREEN`]: the prompt has the rule of the smart trigger.
    screen: bool,
    /// [`prompt::NEEDS_WEB_SEARCH`]: the prompt has the rule for web
    /// search that is switched off ([`AskPlan::web_hint`]).
    web: bool,
    /// Tell the sink when the web search marker is found
    /// ([`AskEvent::Suggestion`]).
    announce_web: bool,
}

impl Watch {
    fn markers(self) -> Vec<hint::Marker> {
        let mut markers = Vec::new();
        if self.screen {
            markers.push(hint::Marker::reply(NEED_SCREEN));
        }
        if self.web {
            markers.push(hint::Marker::line(prompt::NEEDS_WEB_SEARCH));
        }
        markers
    }

    /// A whole reply, read for these markers the way a stream is.
    fn read(self, reply: &str) -> Read {
        let (found, text) = hint::split(&self.markers(), reply);
        Read {
            wants_screen: found.contains(&NEED_SCREEN),
            web: found.contains(&prompt::NEEDS_WEB_SEARCH).then(|| prompt::clean_answer(&text)),
        }
    }
}

/// What a whole reply is, for the markers of one turn ([`Watch::read`]).
struct Read {
    /// The reply is the screen marker.
    wants_screen: bool,
    /// The reply began with the web search marker: the answer that
    /// follows it, tidied. Empty when nothing follows.
    web: Option<String>,
}

/// What an ask has learned about the screen so far.
#[derive(Default)]
struct ScreenSeen {
    state: ScreenState,
    /// For the person; empty unless a capture was refused.
    notice: String,
    /// For the model, when a capture was refused.
    for_model: Option<String>,
}

impl AskPlan {
    /// The plan for these settings, with the dictionary terms given
    /// instead of read from the user's dictionary, and without any
    /// facts about the computer: the prompt only says what day it is.
    /// For tests, and for the eval set run without a device context.
    pub fn with_terms(ask: &AskSettings, refine: &RefineSettings, terms: &[String]) -> Self {
        let today = chrono::Local::now().format("%A %-d %B %Y").to_string();
        Self::assemble(ask, refine, terms, &today, "")
    }

    /// The plan for these settings with the dictionary terms and the
    /// rendered device context given ([`context::DeviceContext::render`]).
    /// The context carries the date when its date group is on, so the
    /// prompt does not say it a second time; with an empty context the
    /// prompt has neither.
    pub fn with_context(ask: &AskSettings, refine: &RefineSettings, terms: &[String], context: &str) -> Self {
        Self::assemble(ask, refine, terms, "", context)
    }

    fn assemble(ask: &AskSettings, refine: &RefineSettings, terms: &[String], today: &str, context: &str) -> Self {
        let ask = ask.clone().cleaned();
        let openai = refine.backend == RefineBackendKind::OpenaiCompatible;
        // Only the Claude CLI has a web search to switch on.
        let web_search = ask.web_search && !openai;
        let notice = if ask.web_search && openai { WEB_SEARCH_UNAVAILABLE } else { "" };
        let timeout_ms = if web_search { ask.timeout_ms.saturating_mul(2) } else { ask.timeout_ms };
        let web = match (web_search, openai) {
            (true, _) => prompt::WebAccess::Search,
            (false, true) => prompt::WebAccess::Unavailable,
            (false, false) => prompt::WebAccess::SwitchedOff,
        };
        let mut plan = Self {
            backend: refine.backend,
            setup: AskSetup {
                // Made below, from the finished plan.
                system: String::new(),
                model: ask.model_for(refine),
                effort: ask.effort_for(refine),
                web_search,
            },
            timeout: Duration::from_millis(timeout_ms.min(ASK_TIMEOUT_MAX_MS * 2)),
            notice: notice.to_string(),
            web_hint: web == prompt::WebAccess::SwitchedOff,
            context: context.trim().to_string(),
            no_long_dashes: refine.no_long_dashes,
            claude_cli: refine.claude_cli.clone(),
            openai: refine.openai.clone(),
            instructions: ask.instructions.clone(),
            terms: terms.to_vec(),
            today: today.to_string(),
            vision: screen::AskVision::default(),
            names_front_app: ask.context.front_app,
            #[cfg(feature = "agent")]
            agent: None,
            tools_prompt: String::new(),
            routed_to: None,
            stand_in: None,
        };
        plan.setup.system = plan.system_for(plan.screen_prompt(false));
        plan
    }

    /// The same plan with the screen in play (TASK-98). A plan starts
    /// without it ([`VisionChoice::None`]): an ask outside the app
    /// never looks at the screen unless its caller says so here.
    ///
    /// - [`VisionChoice::Settings`]: as `vision` says. Its trigger
    ///   decides per question ([`crate::vision::decide`]); to try
    ///   another trigger, pass settings with that trigger set.
    /// - [`VisionChoice::Force`]: this ask takes a screenshot
    ///   (`openwhisper ask run --screen`).
    /// - [`VisionChoice::None`]: no screenshot, whatever `vision` says.
    ///
    /// The never-capture list of `vision` and every other safeguard
    /// of [`crate::vision::capture_for_ask`] apply to all of them.
    ///
    /// [`AskPlan::setup`] becomes the setup a backend is best warmed
    /// with: the prompt for what the settings alone say will happen.
    /// With the phrases trigger that is one prompt for an ask with
    /// and without a screenshot, so the warmed backend answers
    /// whether or not a phrase matched. Only an image the caller
    /// brings to a plan that expected none changes the prompt at
    /// [`AskPlan::send`], and a backend warmed for the other one then
    /// starts over by itself.
    ///
    /// Without the cargo feature `vision` the settings never capture,
    /// and a forced capture ends as "no screenshot" with a reason.
    pub fn with_vision(mut self, vision: &VisionSettings, choice: VisionChoice) -> Self {
        self.vision = screen::AskVision { choice, settings: vision.clone().cleaned() };
        self.setup.system = self.system_for(self.screen_prompt(false));
        self
    }

    /// Whether this plan may include the screen, as given to
    /// [`AskPlan::with_vision`].
    pub fn vision_choice(&self) -> VisionChoice {
        self.vision.choice
    }

    /// The same plan as an ask with tools (Agent, TASK-99): `run` is
    /// what `crate::agent::AgentRun::for_ask` (the app) or
    /// `AgentRun::new` (`openwhisper agent run`) made of the Agent
    /// settings and the app in front. With `None`, and with any
    /// backend but the Claude CLI, the plan stays an ask without
    /// tools.
    ///
    /// What changes:
    ///
    /// - the backend is the CLI started with the run's connections
    ///   and nothing else (`crate::agent::claude`);
    /// - the prompt says which tools there are and what the rules
    ///   for them are (`crate::agent::prompt`);
    /// - the timeout is doubled: an ask that calls tools asks the
    ///   model more than once. The time a confirm waits for the
    ///   person does not count against it;
    /// - when the app in front has one of the run's connections
    ///   ([`crate::agent::AgentRun::route`]), the ask is pointed at
    ///   that connection and takes no screenshot, whatever the
    ///   Vision trigger says, unless the question has one of the
    ///   Vision phrases in it (or the caller forces the screen).
    ///
    /// Call it once, on a plan that has no tools yet.
    #[cfg(feature = "agent")]
    pub fn with_agent(mut self, run: Option<crate::agent::AgentRun>) -> Self {
        let run = run.filter(|_| self.backend != RefineBackendKind::OpenaiCompatible);
        if run.is_some() && self.agent.is_none() {
            let doubled = self.timeout.saturating_mul(2);
            self.timeout = doubled.min(Duration::from_millis(ASK_TIMEOUT_MAX_MS * 2));
        }
        self.tools_prompt = run.as_ref().map_or(String::new(), |run| crate::agent::prompt::section(run, self.names_front_app));
        self.routed_to = run.as_ref().and_then(|run| run.route.as_ref()).map(|route| route.connection.clone());
        self.agent = run;
        self.setup.system = self.system_for(self.screen_prompt(false));
        self
    }

    /// The tools of this plan, when it is an ask with tools.
    #[cfg(feature = "agent")]
    pub fn agent(&self) -> Option<&crate::agent::AgentRun> {
        self.agent.as_ref()
    }

    /// Where this ask will look for what the question is about, as
    /// far as that is known before the question: the connection of
    /// the app in front, when the plan is pointed at one.
    pub fn routed_to(&self) -> Option<&str> {
        self.routed_to.as_deref()
    }

    /// The same plan with `image` standing in for the screen: where
    /// the ask would take a screenshot, it takes this image instead,
    /// and the real screen is never captured. For the eval sets,
    /// which must see what an ask does about the screen without
    /// looking at anybody's.
    #[cfg_attr(not(feature = "agent"), allow(dead_code))]
    pub(crate) fn with_screen_stand_in(mut self, stand_in: screen::StandIn) -> Self {
        self.stand_in = Some(stand_in);
        self
    }

    /// What the prompt says about the screen
    /// ([`screen::AskVision::prompt`]; for an ask that is pointed at
    /// a connection [`screen::AskVision::prompt_routed`]).
    fn screen_prompt(&self, has_image: bool) -> ScreenPrompt {
        if self.routed_to.is_some() { self.vision.prompt_routed(has_image) } else { self.vision.prompt(has_image) }
    }

    /// What this ask can do about the web, as the prompt says it.
    fn web_access(&self) -> prompt::WebAccess {
        match (self.setup.web_search, self.web_hint) {
            (true, _) => prompt::WebAccess::Search,
            (false, true) => prompt::WebAccess::SwitchedOff,
            (false, false) => prompt::WebAccess::Unavailable,
        }
    }

    /// The system prompt of this plan with `screen` for what it says
    /// about the screen: the one place the prompt of a plan is put
    /// together, so the web access, the device context and the dash
    /// rule are the same in the prompt a backend is warmed with and
    /// in the prompt the question goes out with.
    fn system_for(&self, screen: ScreenPrompt) -> String {
        let extras = prompt::Extras {
            context: &self.context,
            no_long_dashes: self.no_long_dashes,
            screen,
            tools: &self.tools_prompt,
        };
        prompt::system_prompt_with(&self.instructions, &self.terms, self.web_access(), &self.today, extras)
    }

    /// The setup of this plan with `screen` for what its prompt says
    /// about the screen, everything else as it is. For the Vision
    /// eval set ([`crate::vision::eval`]), which sends its cases to a
    /// backend itself, so that no eval can reach a capture, and still
    /// has to send the prompt an ask would. Makes a string and
    /// nothing else.
    #[cfg_attr(not(feature = "vision"), allow(dead_code))]
    pub(crate) fn setup_for_screen(&self, screen: ScreenPrompt) -> AskSetup {
        AskSetup { system: self.system_for(screen), ..self.setup.clone() }
    }

    /// The markers a turn under the screen prompt `kind` looks for.
    /// Only where the prompt taught the model a marker can the reply
    /// hold one: the screen marker with the smart trigger, the web
    /// search marker while web search could be switched on. Both can
    /// be in one prompt.
    fn watch_for(&self, kind: ScreenPrompt) -> Watch {
        Watch { screen: kind == ScreenPrompt::OnRequest, web: self.web_hint, announce_web: self.web_hint }
    }

    /// What the app makes of a whole reply to this plan's prompt with
    /// `screen` for what it says about the screen, when the reply
    /// came straight from a backend. Read by the same code as a turn
    /// of an ask ([`Watch::read`]), so the Vision eval set
    /// ([`crate::vision::eval`]) judges exactly what the app would do:
    ///
    /// - a reply the app would take as a request for the screen comes
    ///   back as the bare marker, [`NEED_SCREEN`], however the model
    ///   wrote it;
    /// - any other reply comes back as the person would be shown it:
    ///   without the web search marker, where the prompt asks for
    ///   one, and without long dashes, where those are taken out.
    #[cfg_attr(not(feature = "vision"), allow(dead_code))]
    pub(crate) fn shown(&self, screen: ScreenPrompt, reply: &str) -> String {
        let read = self.watch_for(screen).read(reply);
        if read.wants_screen {
            return NEED_SCREEN.to_string();
        }
        let text = match read.web {
            Some(rest) if rest.is_empty() => NEEDS_THE_WEB.to_string(),
            Some(rest) => rest,
            None => reply.to_string(),
        };
        if self.no_long_dashes { crate::dashes::normalize(&text) } else { text }
    }

    /// The plan for these settings, the current dictionary and the
    /// facts about this computer the settings ask for, read now
    /// ([`context::gather`]). No app in front: for a caller that has
    /// none to name (a terminal is not where the person is working).
    pub fn new(ask: &AskSettings, refine: &RefineSettings) -> Self {
        Self::for_app(ask, refine, None)
    }

    /// [`AskPlan::new`] with the name of the app the person is
    /// working in, when the caller knows it. It reaches the prompt
    /// only while that group of facts is switched on.
    pub fn for_app(ask: &AskSettings, refine: &RefineSettings, front_app: Option<&str>) -> Self {
        let facts = context::gather(&ask.context, front_app).render();
        let matcher = crate::dictionary::current_matcher();
        Self::with_context(ask, refine, matcher.prompt_terms(), &facts)
    }

    /// A backend for this plan. Cheap: nothing is started until
    /// [`AskBackend::warm`] or [`AskBackend::send`].
    pub fn backend(&self) -> Box<dyn AskBackend> {
        #[cfg(test)]
        if let Some(make) = test_support::factory() {
            return make(self);
        }
        #[cfg(feature = "agent")]
        if let Some(run) = &self.agent {
            return Box::new(crate::agent::claude::ClaudeAgent::new(self.claude_cli.clone(), run.clone()));
        }
        match self.backend {
            RefineBackendKind::OpenaiCompatible => Box::new(openai_compat::OpenAiAsk::new(
                self.openai.clone(),
                crate::refine::secrets::api_key().unwrap_or(None),
            )),
            _ => Box::new(claude_cli::ClaudeCliAsk::new(self.claude_cli.clone())),
        }
    }

    /// Where the question is sent, in the words Refine uses for the
    /// same backend.
    pub fn destination(&self) -> String {
        match self.backend {
            RefineBackendKind::OpenaiCompatible => crate::refine::openai_compat::destination(&self.openai),
            _ => crate::refine::claude_cli::DESTINATION.to_string(),
        }
    }

    /// The effort as an id; `""` when none is sent.
    pub fn effort_id(&self) -> &'static str {
        self.setup.effort.map_or("", RefineEffort::as_str)
    }

    /// Ask `question` through `backend`: the question is fenced
    /// ([`prompt::user_message`]), the answer streams into `sink`.
    /// `question.text` is the question as heard or typed.
    ///
    /// With the screen in play ([`AskPlan::with_vision`]) this is
    /// also where the screenshot is taken and sent; see the module
    /// docs. The sink then also gets [`AskEvent::Screen`], and the
    /// reply says what became of the screen ([`AskReply::screen`]).
    /// Images the caller puts in `question.images` are sent as they
    /// are and stand for the screen: the prompt is the one for an ask
    /// with a screenshot, and nothing is captured on top.
    ///
    /// With [`AskPlan::web_hint`], an answer the model marked as
    /// needing the web comes back with [`AskReply::suggestion`] set.
    /// The mark itself reaches neither `sink` nor the reply; the sink
    /// gets [`AskEvent::Suggestion`] in its place.
    pub fn send(
        &self,
        backend: &mut dyn AskBackend,
        question: &AskInput,
        sink: &mut dyn FnMut(AskEvent),
        cancel: &CancelFlag,
    ) -> Result<AskReply, AskError> {
        // A screenshot may only be taken while this guard lives.
        let _ask = screen::OneShot::begin();
        self.converse(None, backend, question, sink, cancel).0
    }

    /// Where an ask of this plan looked, given what became of the
    /// screen: the connection it is pointed at, else the screen when
    /// a screenshot went out (or the caller brought an image), else
    /// nowhere.
    fn context_source(&self, screen: ScreenState, brought_image: bool) -> ContextSource {
        match &self.routed_to {
            Some(name) => ContextSource::Connection { name: name.clone() },
            None if screen == ScreenState::Included || brought_image => ContextSource::Screen,
            None => ContextSource::None,
        }
    }

    /// One ask from the question to the answer: one turn, or two when
    /// the model asked for the screen. `session` is the id of the
    /// app's ask this is, `None` outside the app's session.
    ///
    /// Next to the result: what the ask did besides answering
    /// ([`Trace`]). It is known also when there is no answer, and
    /// that is the point: an ask that took a screenshot or called a
    /// tool and then failed or was cancelled still says so on its
    /// diagnostics row ([`Outcome::record`]).
    fn converse(
        &self,
        session: Option<u64>,
        backend: &mut dyn AskBackend,
        question: &AskInput,
        sink: &mut dyn FnMut(AskEvent),
        cancel: &CancelFlag,
    ) -> (Result<AskReply, AskError>, Trace) {
        if question.text.trim().is_empty() && question.images.is_empty() {
            return (Err(AskError::Backend(NO_QUESTION.to_string())), Trace::default());
        }
        let mut seen = ScreenSeen::default();
        let mut turns = 1;
        // The tool calls, as the backend reports them.
        let mut steps: Vec<AskStep> = Vec::new();
        if let Some(name) = &self.routed_to {
            sink(AskEvent::Context(ContextSource::Connection { name: name.clone() }));
        }
        // The net under the dash rule of the prompt: text passes
        // through it on its way to the sink, so the panel never shows
        // a dash that the finished answer then has not. It sits after
        // the markers are taken off: they are looked for in the text
        // as the model wrote it.
        let mut dashes = self.no_long_dashes.then(crate::dashes::Normalizer::new);
        // The answer as far as it reaches the sink.
        let mut arrived = String::new();
        let result = {
            let mut out = |event: AskEvent| {
                if let AskEvent::Step(step) = &event {
                    steps::upsert(&mut steps, step.clone());
                }
                match (dashes.as_mut(), event) {
                    (Some(dashes), AskEvent::Text(piece)) => {
                        let piece = dashes.push(&piece);
                        if !piece.is_empty() {
                            arrived.push_str(&piece);
                            sink(AskEvent::Text(piece));
                        }
                    }
                    (_, event) => {
                        if let AskEvent::Text(piece) = &event {
                            arrived.push_str(piece);
                        }
                        sink(event);
                    }
                }
            };
            self.exchange(session, backend, question, &mut out, cancel, &mut seen, &mut turns)
        };
        // A dash the text ended on, also when the ask failed midway.
        if let Some(rest) = dashes.as_mut().map(crate::dashes::Normalizer::finish).filter(|rest| !rest.is_empty()) {
            arrived.push_str(&rest);
            sink(AskEvent::Text(rest));
        }
        let result = result.map(|mut reply| {
            if self.no_long_dashes {
                reply.text = crate::dashes::normalize(&reply.text);
            }
            reply
        });
        // Lengths only, never the text: verbose logs end up in bug
        // reports.
        crate::verbose_log!(
            "[ow.ask] backend={} ok={} chars_in={} chars_out={} first_text_ms={:?} ms={:?} turns={} screen={} tools={}",
            backend.id(),
            result.is_ok(),
            question.text.trim().chars().count(),
            result.as_ref().map_or(0, |r| r.text.chars().count()),
            result.as_ref().ok().and_then(|r| r.first_text_ms),
            result.as_ref().ok().map(|r| r.elapsed_ms),
            turns,
            seen.state.as_str(),
            steps.len()
        );
        let trace = Trace {
            screen: seen.state,
            context_source: self.context_source(seen.state, !question.images.is_empty()),
            steps,
            text: arrived,
        };
        let result = result.and_then(|mut reply| {
            if reply.text.is_empty() {
                return Err(AskError::Backend(NO_ANSWER.to_string()));
            }
            // What the backend had to say about this answer, and
            // what was known before the ask began.
            if reply.notice.is_empty() {
                reply.notice = self.notice.clone();
            } else if !self.notice.is_empty() && !reply.notice.contains(&self.notice) {
                reply.notice = format!("{} {}", reply.notice, self.notice);
            }
            reply.screen = seen.state;
            reply.screen_notice = seen.notice;
            reply.steps = trace.steps.clone();
            reply.context_source = trace.context_source.clone();
            Ok(reply)
        });
        (result, trace)
    }

    /// The turns of one ask: the question, and when the model asked
    /// for the screen, the screenshot after it. `seen` and `turns`
    /// are filled in for the caller also when there is no answer.
    #[allow(clippy::too_many_arguments)]
    fn exchange(
        &self,
        session: Option<u64>,
        backend: &mut dyn AskBackend,
        question: &AskInput,
        sink: &mut dyn FnMut(AskEvent),
        cancel: &CancelFlag,
        seen: &mut ScreenSeen,
        turns: &mut usize,
    ) -> Result<AskReply, AskError> {
        let started = Instant::now();
        let mut images = question.images.clone();
        let step = if !images.is_empty() {
            Step::NoCapture
        } else if self.routed_to.is_some() {
            // The app in front has a connection: the screen only
            // when the person asked for it in so many words.
            self.vision.decide_routed(&question.text)
        } else {
            self.vision.decide(&question.text)
        };
        if step == Step::Capture {
            images.extend(self.capture(session, seen, sink));
            if cancel.is_cancelled() {
                return Err(AskError::Cancelled);
            }
        }
        let had_image = !images.is_empty();
        let kind = self.screen_prompt(had_image);
        let setup = AskSetup { system: self.system_for(kind), ..self.setup.clone() };
        let watch = self.watch_for(kind);
        let text = match &seen.for_model {
            Some(why) => prompt::user_message_with_note(&question.text, &prompt::no_screenshot_note(why)),
            None => prompt::user_message(&question.text),
        };
        let (reply, wants_screen) = self.turn(backend, &setup, AskInput { text, images }, sink, cancel, watch)?;
        if !wants_screen {
            return Ok(reply);
        }
        // The model cannot answer without the screen. Capture it now,
        // unless that was already tried for this ask.
        let follow_up = if had_image {
            AskInput::text(prompt::SCREEN_ALREADY_SENT)
        } else if let Some(why) = &seen.for_model {
            AskInput::text(prompt::no_screen_follow_up(why))
        } else {
            match self.capture(session, seen, sink) {
                Some(image) => AskInput { text: prompt::SCREEN_FOLLOWS.to_string(), images: vec![image] },
                None => AskInput::text(prompt::no_screen_follow_up(seen.for_model.as_deref().unwrap_or_default())),
            }
        };
        if cancel.is_cancelled() {
            return Err(AskError::Cancelled);
        }
        *turns = 2;
        let before = started.elapsed();
        // The answer to the second message can be marked as needing
        // the web like any other. The sink hears of that once, also
        // when the first reply already said so before it asked for
        // the screen.
        let watch = Watch { announce_web: watch.announce_web && reply.suggestion.is_none(), ..watch };
        let (second, again) = self.turn(backend, &setup, follow_up, sink, cancel, watch)?;
        if again {
            // Asked for the screen twice: there is no answer.
            return Err(AskError::Backend(NO_ANSWER.to_string()));
        }
        Ok(both_turns(reply, second, before, started.elapsed()))
    }

    /// Take the screenshot for this ask and say on the sink what is
    /// happening. `None` when there is none; `seen` then holds why.
    fn capture(
        &self,
        session: Option<u64>,
        seen: &mut ScreenSeen,
        sink: &mut dyn FnMut(AskEvent),
    ) -> Option<AskImage> {
        sink(AskEvent::Screen { state: ScreenState::Capturing, notice: String::new() });
        if let Some(stand_in) = &self.stand_in {
            // An eval: its own image, and the real screen is left
            // alone.
            seen.state = ScreenState::Included;
            sink(AskEvent::Screen { state: ScreenState::Included, notice: String::new() });
            return Some(stand_in.take());
        }
        // The proof that an ask is asking. Made here, used once.
        let token = match session {
            Some(id) => AskCaptureToken::for_session(id),
            None => AskCaptureToken::for_one_shot(),
        };
        let (image, state, notice, for_model) = match screen::take(&token, &self.vision.settings, self.names_front_app) {
            Shot::Image(image) => (Some(image), ScreenState::Included, String::new(), None),
            Shot::Missing { state, notice, for_model } => (None, state, notice, Some(for_model)),
        };
        seen.state = state;
        seen.notice = notice.clone();
        seen.for_model = for_model;
        sink(AskEvent::Screen { state, notice });
        image
    }

    /// One turn. `input` is taken by value and dropped the moment the
    /// backend call returns: a screenshot in it does not outlive the
    /// request that sent it.
    ///
    /// The markers `watch` names are kept from the sink and taken off
    /// the reply ([`hint`]): one filter for both, because the two
    /// words begin alike. The flag in the result says that the reply
    /// was the screen marker ([`NEED_SCREEN`]); an answer that began
    /// with the web search marker comes back with
    /// [`AskReply::suggestion`] set, and the sink gets
    /// [`AskEvent::Suggestion`] in the marker's place.
    fn turn(
        &self,
        backend: &mut dyn AskBackend,
        setup: &AskSetup,
        input: AskInput,
        sink: &mut dyn FnMut(AskEvent),
        cancel: &CancelFlag,
        watch: Watch,
    ) -> Result<(AskReply, bool), AskError> {
        let markers = watch.markers();
        if markers.is_empty() {
            let result = backend.send(setup, &input, sink, cancel, self.timeout);
            drop(input);
            return result.map(|reply| (reply, false));
        }
        let mut filter = hint::MarkerSink::new(&markers, sink);
        if watch.announce_web {
            filter = filter.announce(prompt::NEEDS_WEB_SEARCH, AskEvent::Suggestion(AskSuggestion::WebSearch));
        }
        let result = backend.send(setup, &input, &mut |event| filter.event(event), cancel, self.timeout);
        drop(input);
        // A turn that failed or was cancelled midway ends here, and
        // the filter with it: what it still holds is at most the
        // start of a marker, which is not shown either. Everything it
        // passed on before stays where it is.
        let mut reply = result?;
        // The turn is complete: what was held back and is no marker.
        let streamed = filter.finish();
        // The reply as a whole is read the same way. It is what
        // counts for a backend that does not stream.
        let read = watch.read(&reply.text);
        let wants_screen = read.wants_screen || streamed.contains(&NEED_SCREEN);
        if let Some(text) = read.web {
            reply.suggestion = Some(AskSuggestion::WebSearch);
            reply.text = text;
            if reply.text.is_empty() && !wants_screen {
                // The marker and nothing after it.
                reply.text = NEEDS_THE_WEB.to_string();
                sink(AskEvent::Text(reply.text.clone()));
            }
        }
        Ok((reply, wants_screen))
    }

    /// [`AskPlan::send`], and then what the app keeps of an ask: its
    /// diagnostics row (when a store is registered,
    /// [`crate::stats::set_store`]) and, with `history`, the question
    /// and the answer in the transcript history (when that is
    /// switched on and has a folder). This is how `openwhisper ask
    /// run` leaves the same trace as the ask key.
    pub fn send_and_record(
        &self,
        backend: &mut dyn AskBackend,
        question: &AskInput,
        sink: &mut dyn FnMut(AskEvent),
        cancel: &CancelFlag,
        history: bool,
    ) -> Result<AskReply, AskError> {
        let at_ms = now_ms();
        let sent = Instant::now();
        let (result, trace) = {
            // A screenshot may only be taken while this guard lives.
            let _ask = screen::OneShot::begin();
            self.converse(None, backend, question, sink, cancel)
        };
        let outcome = Outcome {
            plan: self,
            backend_id: backend.id(),
            question: &question.text,
            result: &result,
            trace: &trace,
            accepted: result.is_ok(),
            at_ms,
            elapsed_ms: sent.elapsed().as_millis() as u64,
            spoken_ms: 0,
        };
        outcome.record(history);
        result
    }
}

/// The reply of an ask that took two turns (the model asked for the
/// screen, then answered): the answer of the second, the time and the
/// tokens of both. `before` is how long after the question the second
/// turn was sent, `total` the whole time.
fn both_turns(first: AskReply, second: AskReply, before: Duration, total: Duration) -> AskReply {
    let usage = match (first.usage, second.usage) {
        (Some(a), Some(b)) => Some(CallUsage {
            tokens_in: a.tokens_in + b.tokens_in,
            tokens_cached: a.tokens_cached + b.tokens_cached,
            tokens_out: a.tokens_out + b.tokens_out,
            // The Claude CLI reports the cost of the conversation so
            // far with every turn, so the later figure holds both.
            cost_usd: match (a.cost_usd, b.cost_usd) {
                (Some(x), Some(y)) => Some(x.max(y)),
                (x, y) => y.or(x),
            },
            plan: b.plan.or(a.plan),
            // Both turns go to the same model; the second answered.
            model: b.model.or(a.model),
        }),
        (a, b) => b.or(a),
    };
    AskReply {
        text: second.text,
        first_text_ms: second.first_text_ms.map(|ms| ms + before.as_millis() as u64),
        elapsed_ms: total.as_millis() as u64,
        usage,
        warm_lead_ms: first.warm_lead_ms,
        notice: if second.notice.is_empty() { first.notice } else { second.notice },
        // The first reply can have said that the web is needed before
        // it asked for the screen.
        suggestion: second.suggestion.or(first.suggestion),
        screen: ScreenState::None,
        screen_notice: String::new(),
        steps: Vec::new(),
        context_source: ContextSource::None,
    }
}

/// Answer one question with a fresh backend: the pure form of an ask,
/// with no global state. `ask` and `refine` are the two settings
/// blocks (the backend, binary path, URL and key are Refine's);
/// `input.text` is the question as heard or typed.
///
/// Does not look at [`AskSettings::enabled`]: a caller that got here
/// decided to ask (`openwhisper ask run` is its own opt-in).
pub fn answer(
    ask: &AskSettings,
    refine: &RefineSettings,
    input: &AskInput,
    sink: &mut dyn FnMut(AskEvent),
    cancel: &CancelFlag,
) -> Result<AskReply, AskError> {
    let plan = AskPlan::new(ask, refine);
    let mut backend = plan.backend();
    plan.send(backend.as_mut(), input, sink, cancel)
}

/// What an ask would use right now, read off the two settings blocks
/// alone: nothing is started and nothing is asked of the backend, so
/// this is cheap enough for anything that only wants to know whether
/// Ask is on.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct AskOutline {
    /// The saved block.
    pub settings: AskSettings,
    /// Id of the backend Refine is set up with, which an ask uses
    /// too: `"claude-cli"` or `"openai-compatible"`.
    pub backend_kind: String,
    /// Where the question is sent, as a sentence part
    /// ([`AskPlan::destination`]).
    pub destination: String,
    /// The model and effort an ask would use right now.
    pub model: String,
    /// `""` when no effort is sent.
    pub effort: String,
    /// Web search is on and the backend has one.
    pub web_search: bool,
    pub timeout_ms: u64,
    /// See [`AskReply::notice`].
    pub notice: String,
}

/// See [`AskOutline`].
pub fn outline(ask: &AskSettings, refine: &RefineSettings) -> AskOutline {
    let plan = AskPlan::with_terms(ask, refine, &[]);
    AskOutline {
        settings: ask.clone().cleaned(),
        backend_kind: plan.backend.as_str().to_string(),
        destination: plan.destination(),
        model: plan.setup.model.clone(),
        effort: plan.effort_id().to_string(),
        web_search: plan.setup.web_search,
        timeout_ms: plan.timeout.as_millis() as u64,
        notice: plan.notice,
    }
}

/// What the Ask pane and `openwhisper ask status` show: the
/// [`AskOutline`] (its fields sit next to `backend` when serialized)
/// and the state of the backend.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct AskStatus {
    #[serde(flatten)]
    pub outline: AskOutline,
    /// Whether the backend can run, and where questions are sent
    /// ([`BackendStatus::destination`]).
    pub backend: BackendStatus,
}

impl std::ops::Deref for AskStatus {
    type Target = AskOutline;

    fn deref(&self) -> &AskOutline {
        &self.outline
    }
}

/// The facts about this computer an ask would send along if it
/// started now, as the model would get them
/// ([`context::DeviceContext::render`]): what `ask` switches on, read
/// this moment, with the app in front as an ask would name it. Empty
/// when there is nothing to tell. For a settings screen that shows
/// the person what leaves the machine; nothing is sent anywhere.
///
/// Asked from one of OpenWhisper's own windows, the app in front is
/// OpenWhisper itself, which an ask never names, so the text then
/// has no such line.
pub fn context_preview(ask: &AskSettings) -> String {
    let ask = ask.clone().cleaned();
    let front_app = ask.context.front_app.then(front_app_name).flatten();
    context::gather(&ask.context, front_app.as_deref()).render()
}

/// Check whether an ask can run with these settings. Makes no model
/// call; for the Claude CLI it runs `claude auth status`, which takes
/// a moment: call [`outline`] when the backend's state is not needed.
pub fn status(ask: &AskSettings, refine: &RefineSettings) -> AskStatus {
    AskStatus { outline: outline(ask, refine), backend: crate::refine::backend_status(refine) }
}

// --- the one ask of the running app --------------------------------

/// Where the current ask stands.
#[non_exhaustive]
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum AskPhase {
    /// No ask.
    #[default]
    Idle,
    /// The question is being recorded or transcribed.
    Listening,
    /// The question is with the model; no text has come back yet.
    Thinking,
    /// The answer is streaming in.
    Answering,
    /// A tool call waits for the person's yes or no
    /// ([`AskSnapshot::confirm`]). Nothing else happens until they
    /// answer, the confirm runs out of time, or the ask is cancelled.
    Confirming,
    /// The answer is complete.
    Done,
    /// There is no answer; [`AskSnapshot::error`] says why.
    Error,
    /// The person cancelled while the model was working.
    Cancelled,
}

impl AskPhase {
    /// Stable id, same as the serialized form.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Listening => "listening",
            Self::Thinking => "thinking",
            Self::Answering => "answering",
            Self::Confirming => "confirming",
            Self::Done => "done",
            Self::Error => "error",
            Self::Cancelled => "cancelled",
        }
    }

    /// Something is in flight that a cancel would stop.
    pub fn is_busy(self) -> bool {
        matches!(self, Self::Listening | Self::Thinking | Self::Answering | Self::Confirming)
    }

    /// The question is with the model: it thinks, answers, or waits
    /// on a confirm.
    pub fn is_running(self) -> bool {
        matches!(self, Self::Thinking | Self::Answering | Self::Confirming)
    }
}

/// The current ask, as the panel shows it.
#[derive(Serialize, Clone, Debug, PartialEq, Default)]
pub struct AskSnapshot {
    /// Tells one ask from the next. 0 before the first.
    pub id: u64,
    pub phase: AskPhase,
    /// The question as it was sent (after rules and dictionary).
    pub question: String,
    /// The answer so far while `answering`, the whole answer when
    /// `done`. Kept when an ask is cancelled or fails midway.
    pub answer: String,
    /// See [`AskReply::notice`].
    pub notice: String,
    /// See [`AskSuggestion`]. Set as soon as the model marked its
    /// answer, so it is there while the answer streams. Not kept in
    /// the history.
    pub suggestion: Option<AskSuggestion>,
    /// Why there is no answer; empty unless the phase is `error`.
    pub error: String,
    /// Backend id, model and effort (`""` when none is sent).
    pub backend: String,
    pub model: String,
    pub effort: String,
    /// Web search is on for this ask.
    pub web_search: bool,
    /// The model is searching the web right now.
    pub searching: bool,
    /// What it is searching for, or last searched for.
    pub search_query: String,
    /// When the ask began (the recording, or the submit for typed
    /// text), Unix epoch milliseconds.
    pub started_at_ms: i64,
    /// Since the question was sent. Counts up while the model works
    /// (it is read off the clock when the snapshot is taken, without
    /// a new `version`), then stays.
    pub elapsed_ms: u64,
    /// From sending the question to the first text.
    pub first_text_ms: Option<u64>,
    pub usage: Option<CallUsage>,
    /// What is happening with the screen in this ask: `capturing`
    /// while the screenshot is taken (show it on the pill),
    /// `included` once one goes out with the question, `blocked` or
    /// `unavailable` when one was wanted and there is none, `none`
    /// otherwise. Stays as it ended when the ask is done.
    pub screen: ScreenState,
    /// Why there is no screenshot, as one sentence for the person.
    /// Empty unless `screen` is `blocked` or `unavailable`.
    pub screen_notice: String,
    /// The tool calls of this ask so far, in the order they were
    /// made (an ask with tools; empty otherwise). A step changes in
    /// place: `running` or `waiting`, then `done`, `failed` or
    /// `denied`. Kept when the ask ends, however it ends.
    pub steps: Vec<AskStep>,
    /// The tool call that waits for the person right now, with its
    /// whole input. Set exactly while the phase is `confirming`.
    pub confirm: Option<AskConfirm>,
    /// Where the ask looked for what the question is about: nowhere,
    /// the screen, or the connection of the app in front.
    pub context_source: ContextSource,
    /// Goes up on every change, so a shell can emit only on change.
    pub version: u64,
}

struct Warm {
    plan: AskPlan,
    backend: Box<dyn AskBackend>,
    /// The context settings the facts in `plan` were read with.
    context_settings: crate::settings::ContextSettings,
    /// The app that was in front when the recording started, as far
    /// as it was read ([`front_app`]): what the tools of the ask are
    /// routed by.
    front: Option<(String, String)>,
}

/// The app the person is working in, as `(id, name)`: `None` when it
/// cannot be read or when it is OpenWhisper itself. Never the title
/// of the window.
#[cfg(feature = "focus")]
fn front_app() -> Option<(String, String)> {
    let focus = crate::focus::current_app();
    let name = focus.app_name.trim().to_string();
    (!focus.is_self && !(focus.app_id.trim().is_empty() && name.is_empty())).then(|| (focus.app_id.trim().to_string(), name))
}

#[cfg(not(feature = "focus"))]
fn front_app() -> Option<(String, String)> {
    None
}

/// Agent is switched on in the settings file as it is right now, so
/// the app in front is worth reading for it.
#[cfg(feature = "agent")]
fn agent_is_on() -> bool {
    crate::settings::fresh_agent_settings().enabled
}

#[cfg(not(feature = "agent"))]
fn agent_is_on() -> bool {
    false
}

/// `plan` as the app's ask uses it: with the tools the Agent settings
/// give it, as they are **on disk** right now
/// ([`crate::settings::fresh_agent_settings`], for the reason
/// [`with_session_vision`] gives: a tool the person just took away
/// from a terminal must be gone for the ask that comes next), and
/// with `front` as the app the person is working in. An ask that is
/// not an agent ask (`crate::agent::AgentRun::for_ask`) comes back
/// unchanged, except for a notice when a connection it would have
/// used has not been looked up since the app started.
#[cfg(feature = "agent")]
fn with_session_agent(plan: AskPlan, front: Option<&(String, String)>) -> AskPlan {
    let settings = crate::settings::fresh_agent_settings();
    let front = front.map(|(id, name)| crate::agent::FrontApp::new(id, name));
    // A connection that has not been looked up in this process is
    // left out, and the ask says so: it does not fail for it.
    let (run, left_out) = crate::agent::AgentRun::for_ask_with_notice(&settings, plan.backend, front.as_ref());
    let mut plan = plan.with_agent(run);
    if !left_out.is_empty() {
        plan.notice = if plan.notice.is_empty() { left_out } else { format!("{} {left_out}", plan.notice) };
    }
    plan
}

#[cfg(not(feature = "agent"))]
fn with_session_agent(plan: AskPlan, _front: Option<&(String, String)>) -> AskPlan {
    plan
}

/// The name of the app the person is working in, for the device
/// context: `None` when it cannot be read or when it is OpenWhisper
/// itself. The name only, never the title of the window.
#[cfg(feature = "focus")]
fn front_app_name() -> Option<String> {
    let focus = crate::focus::current_app();
    let name = focus.app_name.trim();
    (!focus.is_self && !name.is_empty()).then(|| name.to_string())
}

#[cfg(not(feature = "focus"))]
fn front_app_name() -> Option<String> {
    None
}

struct Session {
    snap: AskSnapshot,
    /// When the question went to the backend.
    sent: Option<Instant>,
    /// Stops the request in flight.
    cancel: CancelFlag,
    /// The backend warmed for the recording in flight.
    warm: Option<Warm>,
    /// Voiced length of the recording the question came from.
    spoken_ms: i64,
}

impl Session {
    fn running(&self, id: u64) -> bool {
        self.snap.id == id && self.snap.phase.is_running()
    }

    fn listening(&self, id: u64) -> bool {
        self.snap.id == id && self.snap.phase == AskPhase::Listening
    }

    /// Back to no ask. The id stays, so a late writer of the ask that
    /// was here still finds it, sees the phase and leaves it alone.
    fn reset(&mut self) -> Option<Warm> {
        self.cancel.cancel();
        self.cancel = CancelFlag::new();
        self.sent = None;
        self.spoken_ms = 0;
        self.snap = AskSnapshot { id: self.snap.id, version: self.snap.version, ..AskSnapshot::default() };
        self.warm.take()
    }
}

static SESSION: LazyLock<Mutex<Session>> = LazyLock::new(|| {
    Mutex::new(Session {
        snap: AskSnapshot::default(),
        sent: None,
        cancel: CancelFlag::new(),
        warm: None,
        spoken_ms: 0,
    })
});

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Lock-free mirror of [`AskPhase::is_busy`], for [`is_busy`].
static BUSY: AtomicBool = AtomicBool::new(false);

fn read<R>(f: impl FnOnce(&mut Session) -> R) -> R {
    // A panic while the lock was held left a complete snapshot
    // behind (every write is a few assignments), so poison is ignored.
    let mut guard = SESSION.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut guard)
}

/// Like [`read`], for a change the panel should see.
fn change<R>(f: impl FnOnce(&mut Session) -> R) -> R {
    read(|s| {
        let out = f(s);
        s.snap.version += 1;
        BUSY.store(s.snap.phase.is_busy(), Ordering::Relaxed);
        out
    })
}

fn now_ms() -> i64 {
    crate::history::now_ms()
}

fn switched_off() -> AskError {
    AskError::Unavailable(ASK_OFF.to_string())
}

/// The ask recording started: the phase goes to `listening`. Cheap
/// (no file, no process), so it can run on the hotkey thread; follow
/// it with [`warm_listening`] off that thread. Returns the id of the
/// new ask, which replaces whatever ask was there (a request still
/// running is cancelled).
///
/// With Ask switched off this does nothing and says so.
pub fn mark_listening() -> Result<u64, AskError> {
    let ask = crate::settings::current_ask_settings();
    if !ask.enabled {
        return Err(switched_off());
    }
    let refine = crate::settings::current_refine_settings();
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let old = change(|s| {
        let old = s.reset();
        s.snap = AskSnapshot {
            id,
            phase: AskPhase::Listening,
            backend: refine.backend.as_str().to_string(),
            model: ask.model_for(&refine),
            effort: ask.effort_for(&refine).map_or("", RefineEffort::as_str).to_string(),
            web_search: ask.web_search && refine.backend != RefineBackendKind::OpenaiCompatible,
            started_at_ms: now_ms(),
            version: s.snap.version,
            ..AskSnapshot::default()
        };
        old
    });
    // Outside the lock: dropping a backend kills and reaps a process.
    drop(old);
    Ok(id)
}

/// Let the backend start up while the person talks (the Claude CLI
/// takes over a second to boot; warmed, the first text is back in
/// about half a second). Reads the dictionary and may spawn a
/// process, so call it off the hotkey thread. Does nothing when the
/// ask `id` is no longer listening (a short recording, a quick
/// cancel): a backend that comes up late is dropped.
pub fn warm_listening(id: u64) {
    if !read(|s| s.listening(id)) {
        return;
    }
    let started = Instant::now();
    let ask = session_ask_settings();
    // The facts about the computer are read here, when the person
    // starts to talk: the answer panel never takes focus, so the app
    // in front is the one they are working in. It is read once, for
    // the facts (its name, when that fact is on) and for the tools
    // of an agent ask (which connection belongs to it).
    let front = (ask.context.front_app || agent_is_on()).then(front_app).flatten();
    let front_name =
        front.as_ref().filter(|_| ask.context.front_app).map(|(_, name)| name.clone()).filter(|name| !name.is_empty());
    let plan = with_session_agent(
        with_session_vision(AskPlan::for_app(&ask, &crate::settings::current_refine_settings(), front_name.as_deref())),
        front.as_ref(),
    );
    let mut backend = plan.backend();
    backend.warm(&plan.setup);
    crate::verbose_log!(
        "[ow.ask] warm backend={} setup_ms={} web_search={} (own thread, recording already running)",
        backend.id(),
        started.elapsed().as_millis(),
        plan.setup.web_search
    );
    let mut slot = Some(Warm { plan, backend, context_settings: ask.cleaned().context, front });
    read(|s| {
        if s.listening(id) {
            s.warm = slot.take();
        }
    });
    drop(slot);
}

/// `plan` as the app's ask uses it: with the Vision settings as they
/// are **on disk** right now ([`crate::settings::fresh_vision_settings`]).
///
/// Not the cache: `openwhisper vision config --trigger off` or
/// `never-capture add` from a terminal changes the file while the app
/// runs, and the rule a person just set has to hold for the ask that
/// comes next. Called twice per ask, when the backend is warmed and
/// when the question is known, both off the hotkey thread; the second
/// read is the one the capture is decided and taken with.
fn with_session_vision(plan: AskPlan) -> AskPlan {
    plan.with_vision(&crate::settings::fresh_vision_settings(), VisionChoice::Settings)
}

/// The Ask settings the app's ask runs on: the block this process
/// loaded, with what it tells the model about the computer as the
/// file says right now ([`crate::settings::fresh_ask_context`]), for
/// the same reason as [`with_session_vision`]. Reads a file: not for
/// the hotkey thread.
fn session_ask_settings() -> AskSettings {
    let mut ask = crate::settings::current_ask_settings();
    ask.context = crate::settings::fresh_ask_context();
    ask
}

/// The app's ask `id` is with the model right now. What a capture
/// token of that ask is checked against.
#[cfg_attr(not(feature = "vision"), allow(dead_code))]
pub(crate) fn is_running(id: u64) -> bool {
    read(|s| s.running(id))
}

/// [`mark_listening`] and [`warm_listening`] in one call, on the
/// calling thread: run it off the hotkey thread.
pub fn begin_listening() -> Result<u64, AskError> {
    let id = mark_listening()?;
    warm_listening(id);
    Ok(id)
}

/// Ask `question` (typed text, or a transcript when the caller does
/// not track the recording). Blocks until the ask is over and returns
/// the snapshot it ended with: `done`, `error`, `cancelled`, or `idle`
/// when the question was empty or the ask was dismissed. The phases
/// in between are on [`snapshot`] meanwhile.
///
/// Replaces whatever ask was there. An ask that is `listening` is
/// taken over, with the backend warmed for it.
///
/// With Ask switched off this does nothing and says so.
pub fn submit(question: &str) -> Result<AskSnapshot, AskError> {
    if !crate::settings::current_ask_settings().enabled {
        return Err(switched_off());
    }
    let (id, warm, old) = change(|s| {
        if s.snap.phase == AskPhase::Listening {
            let warm = s.warm.take();
            start(s, question, 0);
            (s.snap.id, warm, None)
        } else {
            let old = s.reset();
            let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
            s.snap.id = id;
            s.snap.started_at_ms = now_ms();
            start(s, question, 0);
            (id, None, old)
        }
    });
    drop(old);
    Ok(run(id, question, warm))
}

/// [`submit`] for the dictation pipeline: the transcript of the ask
/// recording `id` is ready. Goes ahead only when that ask is still
/// listening. When it was cancelled, dismissed or replaced meanwhile,
/// nothing is sent and the result is [`AskError::Cancelled`].
/// `spoken_ms` is the voiced length of the recording, for the history
/// entry.
pub fn submit_listening(id: u64, question: &str, spoken_ms: i64) -> Result<AskSnapshot, AskError> {
    if !crate::settings::current_ask_settings().enabled {
        // Switched off while the person was talking.
        let warm = change(|s| if s.listening(id) { s.reset() } else { None });
        drop(warm);
        return Err(switched_off());
    }
    let warm = change(|s| {
        if !s.listening(id) {
            return Err(AskError::Cancelled);
        }
        let warm = s.warm.take();
        start(s, question, spoken_ms);
        Ok(warm)
    })?;
    Ok(run(id, question, warm))
}

/// The question is known: `thinking`. Under the session lock.
fn start(s: &mut Session, question: &str, spoken_ms: i64) {
    s.cancel = CancelFlag::new();
    s.sent = Some(Instant::now());
    s.spoken_ms = spoken_ms;
    s.snap.question = question.trim().to_string();
    s.snap.phase = AskPhase::Thinking;
}

/// Run the ask `id` to its end. The session is already `thinking`.
///
/// Whatever happens in there, the ask does not stay `thinking`: a
/// panic (in a backend, say) is caught here and ends the ask in
/// `error`, so the panel says so and the next ask can start. The
/// panic hook has run by then, so a crash report is still written.
fn run(id: u64, question: &str, warm: Option<Warm>) -> AskSnapshot {
    let ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_to_end(id, question, warm)));
    if ran.is_err() {
        change(|s| {
            if s.running(id) {
                s.snap.phase = AskPhase::Error;
                s.snap.error = STOPPED.to_string();
                s.snap.searching = false;
                settle_screen(&mut s.snap);
                settle_steps(&mut s.snap);
                s.snap.elapsed_ms = s.sent.take().map_or(0, |t| t.elapsed().as_millis() as u64);
            }
        });
    }
    snapshot()
}

/// The ask ended without an answer: a capture that was under way is
/// not any more.
fn settle_screen(snap: &mut AskSnapshot) {
    if snap.screen == ScreenState::Capturing {
        snap.screen = ScreenState::None;
    }
}

/// The ask ended without an answer: nothing waits for the person any
/// more, and no step stays "running" or "waiting" on a snapshot that
/// will not change again. A call that waited did not run. Of a call
/// that was running nothing more is known: the process it ran in is
/// gone.
fn settle_steps(snap: &mut AskSnapshot) {
    snap.confirm = None;
    for step in &mut snap.steps {
        match step.state {
            StepState::Waiting => {
                step.state = StepState::Denied;
                step.outcome = "The ask ended before you answered, so it was not done.".to_string();
            }
            StepState::Running => {
                step.state = StepState::Failed;
                step.outcome = "The ask ended while this was running, so its result is not known.".to_string();
            }
            _ => {}
        }
    }
}

/// The phase an ask goes back to when a confirm is over: `answering`
/// when text has already arrived, `thinking` otherwise.
fn after_confirm(snap: &AskSnapshot) -> AskPhase {
    if snap.answer.trim().is_empty() { AskPhase::Thinking } else { AskPhase::Answering }
}

/// [`run`] without the net under it.
fn run_to_end(id: u64, question: &str, warm: Option<Warm>) {
    let question = question.trim();
    if question.is_empty() {
        // Nothing was heard: no call, no error, no ask.
        drop(warm);
        let late = change(|s| if s.running(id) { s.reset() } else { None });
        drop(late);
        return;
    }
    let ask = session_ask_settings().cleaned();
    let refine = crate::settings::current_refine_settings();
    // The facts about the computer are the ones read when the
    // recording started (the time then, the app in front then), as
    // long as the switches are what they were: the backend was warmed
    // with a prompt that holds them. A typed question, or a recording
    // too short for the warm-up to finish, reads them now.
    let facts = match &warm {
        Some(warm) if warm.context_settings == ask.context => warm.plan.context.clone(),
        _ => context_preview(&ask),
    };
    let terms = crate::dictionary::current_matcher();
    // The app in front, for the tools of an agent ask: the one read
    // when the recording started, like the facts. A typed question
    // reads it now.
    let front = match &warm {
        Some(warm) => warm.front.clone(),
        None => agent_is_on().then(front_app).flatten(),
    };
    let plan = with_session_agent(
        with_session_vision(AskPlan::with_context(&ask, &refine, terms.prompt_terms(), &facts)),
        front.as_ref(),
    );
    // The backend warmed while the person talked is used when it was
    // warmed for exactly this plan, which is every ask where no
    // setting or dictionary term changed meanwhile.
    let mut backend = match warm {
        Some(warm) if warm.plan == plan => warm.backend,
        stale => {
            drop(stale);
            plan.backend()
        }
    };
    let backend_id = backend.id();
    let going = change(|s| {
        s.running(id).then(|| {
            s.snap.backend = backend_id.to_string();
            s.snap.model = plan.setup.model.clone();
            s.snap.effort = plan.effort_id().to_string();
            s.snap.web_search = plan.setup.web_search;
            s.snap.notice = plan.notice.clone();
            (s.cancel.clone(), s.snap.started_at_ms, s.spoken_ms)
        })
    });
    let Some((cancel, started_at_ms, spoken_ms)) = going else {
        // Cancelled or replaced before the backend was even ready.
        return;
    };

    let sent = Instant::now();
    let mut sink = |event: AskEvent| {
        change(|s| {
            if !s.running(id) {
                return;
            }
            match event {
                AskEvent::Text(piece) => {
                    if s.snap.first_text_ms.is_none() {
                        s.snap.first_text_ms = Some(sent.elapsed().as_millis() as u64);
                    }
                    s.snap.answer.push_str(&piece);
                    s.snap.phase = AskPhase::Answering;
                    s.snap.searching = false;
                }
                AskEvent::Searching { query } => {
                    s.snap.searching = true;
                    s.snap.search_query = query;
                }
                AskEvent::SearchDone => s.snap.searching = false,
                AskEvent::Screen { state, notice } => {
                    s.snap.screen = state;
                    s.snap.screen_notice = notice;
                    if state == ScreenState::Included && s.snap.context_source == ContextSource::None {
                        s.snap.context_source = ContextSource::Screen;
                    }
                }
                AskEvent::Suggestion(suggestion) => s.snap.suggestion = Some(suggestion),
                AskEvent::Step(step) => steps::upsert(&mut s.snap.steps, step),
                AskEvent::Confirm(confirm) => {
                    s.snap.confirm = Some(confirm);
                    s.snap.phase = AskPhase::Confirming;
                    s.snap.searching = false;
                }
                AskEvent::ConfirmClosed { request_id, .. } => {
                    // `confirm_allow` and `confirm_deny` have
                    // usually done this already; a confirm that ran
                    // out of time ends here.
                    if s.snap.confirm.as_ref().is_some_and(|c| c.request_id == request_id) {
                        s.snap.confirm = None;
                    }
                    if s.snap.phase == AskPhase::Confirming && s.snap.confirm.is_none() {
                        s.snap.phase = after_confirm(&s.snap);
                    }
                }
                AskEvent::Context(source) => s.snap.context_source = source,
            }
        });
    };
    let (result, trace) = plan.converse(Some(id), backend.as_mut(), &AskInput::text(question), &mut sink, &cancel);
    // One ask is one conversation: the process goes with the answer.
    drop(backend);
    let elapsed_ms = sent.elapsed().as_millis() as u64;

    // The answer counts only when the ask is still the one on show:
    // not cancelled, dismissed or replaced in its last moment.
    let accepted = change(|s| {
        if s.snap.id != id {
            return false;
        }
        match s.snap.phase {
            AskPhase::Thinking | AskPhase::Answering | AskPhase::Confirming => {
                s.snap.searching = false;
                s.snap.elapsed_ms = elapsed_ms;
                s.sent = None;
                s.snap.confirm = None;
                match &result {
                    Ok(reply) => {
                        s.snap.steps = reply.steps.clone();
                        s.snap.context_source = reply.context_source.clone();
                        s.snap.answer = reply.text.clone();
                        s.snap.notice = reply.notice.clone();
                        s.snap.suggestion = reply.suggestion;
                        s.snap.first_text_ms = s.snap.first_text_ms.or(reply.first_text_ms);
                        s.snap.usage = reply.usage.clone();
                        s.snap.screen = reply.screen;
                        s.snap.screen_notice = reply.screen_notice.clone();
                        s.snap.phase = AskPhase::Done;
                        true
                    }
                    Err(AskError::Cancelled) => {
                        settle_screen(&mut s.snap);
                        settle_steps(&mut s.snap);
                        s.snap.phase = AskPhase::Cancelled;
                        false
                    }
                    Err(e) => {
                        settle_screen(&mut s.snap);
                        settle_steps(&mut s.snap);
                        s.snap.error = error_text(e, &plan);
                        s.snap.phase = AskPhase::Error;
                        false
                    }
                }
            }
            // `cancel` already put the phase there; the time is ours.
            AskPhase::Cancelled => {
                s.snap.elapsed_ms = elapsed_ms;
                s.sent = None;
                false
            }
            _ => false,
        }
    });

    let outcome = Outcome {
        plan: &plan,
        backend_id,
        question,
        result: &result,
        trace: &trace,
        accepted,
        at_ms: started_at_ms,
        elapsed_ms,
        spoken_ms,
    };
    outcome.record(true);
}

/// How one ask ended, for what is kept of it. The one path for the
/// app's ask and for [`AskPlan::send_and_record`].
struct Outcome<'a> {
    plan: &'a AskPlan,
    backend_id: &'a str,
    /// As it was sent, before the fence.
    question: &'a str,
    result: &'a Result<AskReply, AskError>,
    /// What the ask did besides answering, known also when there is
    /// no reply.
    trace: &'a Trace,
    /// The answer counted: it was not cancelled, dismissed or
    /// replaced in its last moment.
    accepted: bool,
    /// When the ask began, Unix epoch milliseconds.
    at_ms: i64,
    elapsed_ms: u64,
    /// Voiced length of the recording; 0 for a typed question.
    spoken_ms: i64,
}

impl Outcome<'_> {
    /// Write the diagnostics row and, with `history`, keep the ask
    /// in the transcript history: an accepted answer always, and an
    /// ask that failed, timed out or was cancelled when a tool had
    /// run in it by then (with what there is of the answer, marked
    /// as incomplete, and how each call stood).
    fn record(&self, history: bool) {
        let status = match self.result {
            Ok(_) if self.accepted => "answered",
            Ok(_) | Err(AskError::Cancelled) => "cancelled",
            Err(AskError::Timeout) => "timeout",
            Err(AskError::Unavailable(_)) => "unavailable",
            Err(AskError::Backend(_)) => "failed",
        };
        runs::record_run(&run_record(
            status,
            self.plan,
            self.backend_id,
            self.question,
            self.result,
            self.trace,
            self.at_ms,
            self.elapsed_ms,
        ));
        if !history {
            return;
        }
        if let (true, Ok(reply)) = (self.accepted, self.result) {
            let screen = reply.screen == ScreenState::Included;
            keep_in_history(self.question, &reply.text, self.at_ms, self.spoken_ms, screen, &reply.steps);
            return;
        }
        // No answer that counted. An ask without tools then leaves
        // nothing. One in which a tool ran, or may have run, is kept
        // all the same: something was done in the person's name, and
        // the history is where they look for what.
        let ran = |step: &AskStep| matches!(step.state, StepState::Running | StepState::Done | StepState::Failed);
        if !self.trace.steps.iter().any(ran) {
            return;
        }
        let (so_far, why) = match self.result {
            Ok(reply) => (reply.text.trim(), "The ask was cancelled before it finished.".to_string()),
            Err(AskError::Cancelled) => (self.trace.text.trim(), "The ask was cancelled before it finished.".to_string()),
            Err(e) => (self.trace.text.trim(), format!("The ask did not finish: {}", error_text(e, self.plan))),
        };
        let answer = if so_far.is_empty() { format!("(No answer. {why})") } else { format!("{so_far}\n\n(Incomplete. {why})") };
        let screen = self.trace.screen == ScreenState::Included;
        keep_in_history(self.question, &answer, self.at_ms, self.spoken_ms, screen, &self.trace.steps);
    }
}

/// What the panel says for an error.
fn error_text(e: &AskError, plan: &AskPlan) -> String {
    match e {
        AskError::Timeout => format!("No answer within {} seconds.", plan.timeout.as_secs()),
        other => other.to_string(),
    }
}

/// The diagnostics row of one ask: a row in `refine_runs` with source
/// [`runs::SOURCE_ASK`], so the plan usage estimate covers asks too.
/// Lengths, timings and tokens; never the question or the answer.
///
/// And whether a screenshot was taken (`screen`): that is on the row
/// of every ask, also one that failed or was cancelled after the
/// capture. The transcript history only keeps asks that finished, so
/// this row is the record that a screenshot existed at all. The same
/// goes for the tools of an agent ask: how many calls were made, and
/// where the ask looked for its context (never which tools, their
/// input or their results).
#[allow(clippy::too_many_arguments)]
fn run_record(
    status: &str,
    plan: &AskPlan,
    backend_id: &str,
    question: &str,
    result: &Result<AskReply, AskError>,
    trace: &Trace,
    at_ms: i64,
    elapsed_ms: u64,
) -> RefineRun {
    let reply = result.as_ref().ok();
    let usage = reply.and_then(|r| r.usage.as_ref());
    let windows = usage.and_then(|u| u.plan.as_ref());
    RefineRun {
        at_ms,
        source: runs::SOURCE_ASK.to_string(),
        status: status.to_string(),
        backend: backend_id.to_string(),
        model: plan.setup.model.clone(),
        effort: plan.effort_id().to_string(),
        elapsed_ms: elapsed_ms as i64,
        warm_lead_ms: reply.and_then(|r| r.warm_lead_ms).map(|ms| ms as i64),
        chars_in: question.trim().chars().count() as i64,
        chars_out: reply.map_or(0, |r| r.text.chars().count() as i64),
        instruction: false,
        detail: match result {
            Ok(_) => String::new(),
            Err(e) => error_text(e, plan),
        },
        tokens_in: usage.map(|u| u.tokens_in as i64),
        tokens_cached: usage.map(|u| u.tokens_cached as i64),
        tokens_out: usage.map(|u| u.tokens_out as i64),
        cost_usd: usage.and_then(|u| u.cost_usd),
        window_5h_used: windows.and_then(|p| p.five_hour_used),
        window_5h_resets_at: windows.and_then(|p| p.five_hour_resets_at),
        window_7d_used: windows.and_then(|p| p.seven_day_used),
        window_7d_resets_at: windows.and_then(|p| p.seven_day_resets_at),
        mode: None,
        model_exact: usage.and_then(|u| u.model.clone()),
        first_text_ms: reply.and_then(|r| r.first_text_ms).map(|ms| ms as i64),
        screen: Some(trace.screen == ScreenState::Included),
        context_source: Some(trace.context_source.label()),
        tool_calls: Some(trace.steps.len() as i64),
    }
}

/// With transcript history on, keep the ask: the question as the
/// entry's text, the answer under it ([`crate::history`]). A
/// cancelled or failed ask is kept only when a tool ran in it
/// ([`Outcome::record`] decides, and makes the answer text say that
/// it is incomplete). `screen`: a screenshot went out with the
/// question, which the entry says (the image itself is kept
/// nowhere). `steps`: the tool calls of an agent ask, of which the
/// entry keeps the tool and how the call ended, never its input or
/// its result. A call that was still running when the ask ended is
/// kept as `result_not_known`; one that still waited for a yes did
/// not run, and is kept as `denied`.
fn keep_in_history(question: &str, answer: &str, at_ms: i64, spoken_ms: i64, screen: bool, steps: &[AskStep]) {
    let settings = crate::settings::current_history_settings();
    let Some(dir) = crate::history::current_dir() else {
        return;
    };
    let tools: Vec<crate::history::HistoryTool> = steps
        .iter()
        .map(|step| crate::history::HistoryTool {
            connection: step.connection.clone(),
            tool: step.tool.clone(),
            outcome: match step.state {
                StepState::Running => crate::history::TOOL_RESULT_NOT_KNOWN.to_string(),
                StepState::Waiting => StepState::Denied.as_str().to_string(),
                state => state.as_str().to_string(),
            },
        })
        .collect();
    if settings.enabled {
        let entry = crate::history::NewEntry {
            at_ms: if at_ms > 0 { at_ms } else { now_ms() },
            duration_ms: spoken_ms,
            text: question,
            original: None,
            app_id: "",
            app_name: "",
            mode: "",
            refined: "",
            effort: "",
            kind: crate::history::KIND_ASK,
            answer: Some(answer),
            screen,
            tools: &tools,
        };
        if let Err(e) = crate::history::append(&dir, &entry) {
            eprintln!("[history] could not save the ask: {e}");
        }
    }
    // Retention applies with history on or off.
    if let Err(e) = crate::history::prune(&dir, &settings, now_ms()) {
        eprintln!("[history] prune failed: {e}");
    }
}

/// Stop the current ask. While the model is working, the request is
/// killed and the phase goes to `cancelled` at once (what has arrived
/// of the answer stays on the snapshot). A tool call that was waiting
/// for a yes or no is refused, and never runs. While `listening`, the ask
/// is simply gone: back to `idle`, and the transcript of that
/// recording will not be sent. In any other phase nothing happens.
/// Returns whether there was something to stop.
///
/// This stops whatever ask is there now, which is what the cancel
/// key means. A caller that has one particular ask in mind uses
/// [`cancel_if`].
pub fn cancel() -> bool {
    cancel_current(None)
}

/// [`cancel`], but only when the current ask is still the ask `id`.
/// An ask that started after the caller last looked is not touched.
pub fn cancel_if(id: u64) -> bool {
    cancel_current(Some(id))
}

fn cancel_current(only: Option<u64>) -> bool {
    let (stopped, warm) = change(|s| {
        if only.is_some_and(|id| id != s.snap.id) {
            return (false, None);
        }
        match s.snap.phase {
            AskPhase::Listening => (true, s.reset()),
            AskPhase::Thinking | AskPhase::Answering | AskPhase::Confirming => {
                // A confirm that waits is a no from here on: the
                // backend sees the flag, tells the CLI so, and ends
                // the process.
                s.cancel.cancel();
                s.snap.phase = AskPhase::Cancelled;
                s.snap.searching = false;
                settle_screen(&mut s.snap);
                settle_steps(&mut s.snap);
                s.snap.elapsed_ms = s.sent.take().map_or(0, |t| t.elapsed().as_millis() as u64);
                (true, None)
            }
            _ => (false, None),
        }
    });
    drop(warm);
    stopped
}

/// Close the ask: cancel it when it is still running, then `idle`
/// with nothing on the snapshot.
///
/// This closes whatever ask is there now. The panel's close button,
/// and anything else that closes the ask it was showing, uses
/// [`dismiss_if`].
pub fn dismiss() {
    let warm = change(Session::reset);
    drop(warm);
}

/// [`dismiss`], but only when the current ask is still the ask `id`:
/// a close that arrives late (the panel closes a cancelled ask a
/// moment after showing it; a recording turns out empty) must not
/// take a new ask with it. Returns whether that ask was closed.
pub fn dismiss_if(id: u64) -> bool {
    let (closed, warm) = change(|s| if s.snap.id == id { (true, s.reset()) } else { (false, None) });
    drop(warm);
    closed
}

/// The recording of the ask `id` ended without a question to send:
/// it could not be transcribed. The ask ends in `error` with `reason`
/// on the snapshot, so the panel says what happened instead of
/// listening forever. Only while that ask is still `listening`;
/// returns whether it was.
pub fn fail_listening(id: u64, reason: &str) -> bool {
    let (failed, warm) = change(|s| {
        if !s.listening(id) {
            return (false, None);
        }
        s.snap.phase = AskPhase::Error;
        s.snap.error = reason.trim().to_string();
        (true, s.warm.take())
    });
    drop(warm);
    failed
}

/// Say yes to the confirm `request_id` of the ask `ask_id`: the tool
/// runs, with exactly the input the confirm showed. Only while that
/// ask is the current one and that confirm is the one it is waiting
/// on ([`AskSnapshot::confirm`]); returns whether it was. A click
/// that arrives late (the confirm ran out of time, the ask was
/// cancelled or replaced) changes nothing, and neither does a second
/// answer to the same confirm.
///
/// Neither does a click that arrives **too early**: a yes within
/// [`CONFIRM_ARM_MS`] of the confirm coming up was on its way before
/// the person could have read it (a click meant for the confirm
/// before, a double click). It is refused, returns `false`, and the
/// confirm stays open. [`AskConfirm::armed_in_ms`] on the snapshot
/// says how long that still is, so the panel can show the button as
/// not yet active.
pub fn confirm_allow(ask_id: u64, request_id: &str) -> bool {
    confirm_answer(ask_id, request_id, ConfirmDecision::Allow)
}

/// Say no to the confirm `request_id` of the ask `ask_id`: the tool
/// does not run, the model is told so, and the ask goes on to its
/// answer. Scoped like [`confirm_allow`].
pub fn confirm_deny(ask_id: u64, request_id: &str) -> bool {
    confirm_answer(ask_id, request_id, ConfirmDecision::Deny)
}

fn confirm_answer(ask_id: u64, request_id: &str, decision: ConfirmDecision) -> bool {
    change(|s| {
        let on_show = s.snap.id == ask_id
            && s.snap.phase == AskPhase::Confirming
            && s.snap.confirm.as_ref().is_some_and(|c| c.request_id == request_id);
        // A person's answer: a yes that comes before the confirm
        // has been on show for a moment is refused, and the confirm
        // stays open (`steps::CONFIRM_ARM_MS`).
        if !on_show || steps::answer_confirm_as_person(request_id, decision) != steps::ConfirmAnswer::Taken {
            return false;
        }
        // At once, so the panel closes the confirm and a second
        // click finds nothing to answer. What became of the step
        // follows from the backend a moment later.
        s.snap.confirm = None;
        s.snap.phase = after_confirm(&s.snap);
        true
    })
}

/// The current ask.
pub fn snapshot() -> AskSnapshot {
    read(|s| {
        let mut snap = s.snap.clone();
        if let Some(sent) = s.sent.filter(|_| snap.phase.is_running()) {
            snap.elapsed_ms = sent.elapsed().as_millis() as u64;
        }
        // Read off the clock like the elapsed time: how long until a
        // yes to the confirm counts.
        if let Some(confirm) = snap.confirm.as_mut() {
            confirm.armed_in_ms = steps::confirm_armed_in_ms(&confirm.request_id).unwrap_or(0);
        }
        snap
    })
}

/// An ask is listening, thinking or answering: a cancel would stop
/// something. Lock-free, so a keyboard hook can gate the cancel key
/// on it.
pub fn is_busy() -> bool {
    BUSY.load(Ordering::Relaxed)
}

/// The complete answer, only once the ask is `done`. This is what an
/// explicit "insert" hands on; core never does so itself.
pub fn finished_answer() -> Option<String> {
    read(|s| (s.snap.phase == AskPhase::Done).then(|| s.snap.answer.clone()))
}

/// [`finished_answer`] together with the id of the ask it is the
/// answer of, so the caller can close exactly that ask before it
/// hands the text on.
pub fn finished() -> Option<(u64, String)> {
    read(|s| (s.snap.phase == AskPhase::Done).then(|| (s.snap.id, s.snap.answer.clone())))
}

/// What an explicit "copy" copies: the whole answer once it is
/// complete, what has arrived of it before that (also of an ask that
/// was cancelled or failed midway). `None` when there is no text at
/// all.
pub fn text_to_copy() -> Option<String> {
    read(|s| Some(s.snap.answer.clone()).filter(|text| !text.trim().is_empty()))
}

// --- test support --------------------------------------------------

/// Stub backend and factory override for unit tests in this crate.
#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use std::sync::MutexGuard;

    type Factory = Arc<dyn Fn(&AskPlan) -> Box<dyn AskBackend> + Send + Sync>;

    static FACTORY: Mutex<Option<Factory>> = Mutex::new(None);

    pub(crate) fn factory() -> Option<Factory> {
        FACTORY.lock().ok().and_then(|g| g.clone())
    }

    /// Holds the global test lock and resets shared state on drop.
    pub(crate) struct Scope {
        _guard: MutexGuard<'static, ()>,
    }

    impl Drop for Scope {
        fn drop(&mut self) {
            super::dismiss();
            if let Ok(mut g) = FACTORY.lock() {
                *g = None;
            }
            crate::settings::set_ask_settings_cache(AskSettings::default());
            crate::settings::set_refine_settings_cache(RefineSettings::default());
            crate::settings::set_vision_settings_cache(VisionSettings::default());
            crate::settings::set_agent_settings_cache(crate::settings::AgentSettings::default());
            crate::settings::set_settings_file(None);
        }
    }

    /// Install `make` as the backend factory and `settings` as the
    /// current Ask settings for the lifetime of the returned scope.
    /// Refine is at its defaults (the Claude backend).
    ///
    /// Holds the lock that serializes tests touching the process-wide
    /// ask, the settings caches or the factory override.
    pub(crate) fn scope(
        settings: AskSettings,
        make: impl Fn(&AskPlan) -> Box<dyn AskBackend> + Send + Sync + 'static,
    ) -> Scope {
        let guard = crate::settings::CACHE_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        super::dismiss();
        if let Ok(mut g) = FACTORY.lock() {
            *g = Some(Arc::new(make));
        }
        crate::settings::set_ask_settings_cache(settings);
        crate::settings::set_refine_settings_cache(RefineSettings::default());
        crate::settings::set_vision_settings_cache(VisionSettings::default());
        crate::settings::set_agent_settings_cache(crate::settings::AgentSettings::default());
        // No settings file: the asks of a test run on the caches.
        crate::settings::set_settings_file(None);
        Scope { _guard: guard }
    }

    /// The process-wide store ([`crate::stats::store`]), in memory,
    /// registered on first use, so a test can read the diagnostics
    /// rows an ask leaves. Tests elsewhere in the crate then write
    /// their stats and Refine rows into it as well, which harms
    /// nothing.
    pub(crate) fn store() -> &'static crate::store::Store {
        crate::stats::set_store(Arc::new(crate::store::Store::in_memory_for_test()));
        crate::stats::store().expect("a store is registered")
    }

    /// What the [`Stub`]s of one test saw and did.
    #[derive(Default, Debug)]
    pub(crate) struct Calls {
        pub made: usize,
        pub warmed: usize,
        pub sent: Vec<String>,
        pub dropped: usize,
        pub last_system: String,
        pub last_images: usize,
        /// Per turn: the system prompt and the sizes of the images.
        pub systems: Vec<String>,
        pub images: Vec<Vec<usize>>,
    }

    type Script =
        Arc<dyn Fn(&AskInput, &mut dyn FnMut(AskEvent), &CancelFlag) -> Result<String, AskError> + Send + Sync>;

    /// Backend that answers from a closure and records its calls. The
    /// closure streams through the sink like a real backend and
    /// returns the whole answer.
    pub(crate) struct Stub {
        pub calls: Arc<Mutex<Calls>>,
        pub script: Script,
    }

    impl Stub {
        pub(crate) fn new(
            calls: Arc<Mutex<Calls>>,
            script: impl Fn(&AskInput, &mut dyn FnMut(AskEvent), &CancelFlag) -> Result<String, AskError>
            + Send
            + Sync
            + 'static,
        ) -> Self {
            calls.lock().unwrap().made += 1;
            Self { calls, script: Arc::new(script) }
        }

        /// A stub that streams `answer` in two pieces.
        pub(crate) fn saying(calls: Arc<Mutex<Calls>>, answer: &'static str) -> Self {
            Self::new(calls, move |_, sink, _| {
                let half = answer.char_indices().nth(answer.chars().count() / 2).map_or(0, |(i, _)| i);
                sink(AskEvent::Text(answer[..half].to_string()));
                sink(AskEvent::Text(answer[half..].to_string()));
                Ok(answer.to_string())
            })
        }
    }

    impl AskBackend for Stub {
        fn id(&self) -> &'static str {
            "stub"
        }
        fn warm(&mut self, _setup: &AskSetup) {
            self.calls.lock().unwrap().warmed += 1;
        }
        fn send(
            &mut self,
            setup: &AskSetup,
            input: &AskInput,
            sink: &mut dyn FnMut(AskEvent),
            cancel: &CancelFlag,
            _timeout: Duration,
        ) -> Result<AskReply, AskError> {
            {
                let mut c = self.calls.lock().unwrap();
                c.sent.push(input.text.clone());
                c.last_system = setup.system.clone();
                c.last_images = input.images.len();
                c.systems.push(setup.system.clone());
                c.images.push(input.images.iter().map(|image| image.data.len()).collect());
            }
            let text = (self.script)(input, sink, cancel)?;
            Ok(AskReply {
                text: text.trim().to_string(),
                first_text_ms: Some(120),
                elapsed_ms: 480,
                usage: Some(CallUsage { tokens_in: 900, tokens_cached: 800, tokens_out: 40, cost_usd: Some(0.002), plan: None, model: None }),
                warm_lead_ms: Some(2500),
                notice: String::new(),
                ..AskReply::default()
            })
        }
    }

    impl Drop for Stub {
        fn drop(&mut self) {
            if let Ok(mut c) = self.calls.lock() {
                c.dropped += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests;
#[cfg(all(test, feature = "vision"))]
mod screen_tests;
#[cfg(all(test, feature = "agent"))]
mod agent_tests;
