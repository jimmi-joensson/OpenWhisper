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
//!   reply is not it.
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
//!   there now" (the cancel key).
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
pub mod eval;
pub mod openai_compat;
pub mod prompt;
mod screen;

pub use crate::refine::CallUsage;
pub use prompt::ScreenPrompt;
pub use screen::{AskCaptureToken, NEED_SCREEN, ScreenState, VisionChoice};

use crate::settings::VisionSettings;
use screen::{Shot, Step};

// --- what goes in and what comes out -------------------------------

/// An image sent along with a question: the screenshot of an ask
/// that includes the screen (TASK-98). It lives until the backend
/// call that sends it returns.
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
    claude_cli: ClaudeCliSettings,
    openai: OpenAiSettings,
    /// What the system prompt is made of, to make it again once it is
    /// known whether a screenshot goes along.
    instructions: String,
    terms: Vec<String>,
    today: String,
    /// Whether and when this ask includes the screen.
    vision: screen::AskVision,
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
    /// instead of read from the user's dictionary. For the eval set
    /// and tests.
    pub fn with_terms(ask: &AskSettings, refine: &RefineSettings, terms: &[String]) -> Self {
        let ask = ask.clone().cleaned();
        let openai = refine.backend == RefineBackendKind::OpenaiCompatible;
        // Only the Claude CLI has a web search to switch on.
        let web_search = ask.web_search && !openai;
        let notice = if ask.web_search && openai { WEB_SEARCH_UNAVAILABLE } else { "" };
        let timeout_ms = if web_search { ask.timeout_ms.saturating_mul(2) } else { ask.timeout_ms };
        let today = chrono::Local::now().format("%A %-d %B %Y").to_string();
        Self {
            backend: refine.backend,
            setup: AskSetup {
                system: prompt::system_prompt(&ask.instructions, terms, web_search, &today),
                model: ask.model_for(refine),
                effort: ask.effort_for(refine),
                web_search,
            },
            timeout: Duration::from_millis(timeout_ms.min(ASK_TIMEOUT_MAX_MS * 2)),
            notice: notice.to_string(),
            claude_cli: refine.claude_cli.clone(),
            openai: refine.openai.clone(),
            instructions: ask.instructions.clone(),
            terms: terms.to_vec(),
            today,
            vision: screen::AskVision::default(),
        }
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
    /// When a question turns out otherwise (a phrase matched, so a
    /// screenshot goes along after all), [`AskPlan::send`] uses the
    /// prompt for that, and a backend warmed for the other one starts
    /// over by itself.
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

    /// What the prompt says about the screen: by what the settings
    /// alone say will happen, or, when they expect no screenshot and
    /// there is one after all, the prompt for a screenshot.
    fn screen_prompt(&self, has_image: bool) -> ScreenPrompt {
        match self.vision.expected() {
            Step::AskModel => ScreenPrompt::OnRequest,
            Step::Capture => ScreenPrompt::Attached,
            Step::NoCapture if has_image => ScreenPrompt::Attached,
            Step::NoCapture => ScreenPrompt::None,
        }
    }

    fn system_for(&self, screen: ScreenPrompt) -> String {
        prompt::system_prompt_for(&self.instructions, &self.terms, self.setup.web_search, &self.today, screen)
    }

    /// The plan for these settings and the current dictionary.
    pub fn new(ask: &AskSettings, refine: &RefineSettings) -> Self {
        let matcher = crate::dictionary::current_matcher();
        Self::with_terms(ask, refine, matcher.prompt_terms())
    }

    /// A backend for this plan. Cheap: nothing is started until
    /// [`AskBackend::warm`] or [`AskBackend::send`].
    pub fn backend(&self) -> Box<dyn AskBackend> {
        #[cfg(test)]
        if let Some(make) = test_support::factory() {
            return make(self);
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
    pub fn send(
        &self,
        backend: &mut dyn AskBackend,
        question: &AskInput,
        sink: &mut dyn FnMut(AskEvent),
        cancel: &CancelFlag,
    ) -> Result<AskReply, AskError> {
        // A screenshot may only be taken while this guard lives.
        let _ask = screen::OneShot::begin();
        self.converse(None, backend, question, sink, cancel)
    }

    /// One ask from the question to the answer: one turn, or two when
    /// the model asked for the screen. `session` is the id of the
    /// app's ask this is, `None` outside the app's session.
    fn converse(
        &self,
        session: Option<u64>,
        backend: &mut dyn AskBackend,
        question: &AskInput,
        sink: &mut dyn FnMut(AskEvent),
        cancel: &CancelFlag,
    ) -> Result<AskReply, AskError> {
        if question.text.trim().is_empty() && question.images.is_empty() {
            return Err(AskError::Backend(NO_QUESTION.to_string()));
        }
        let started = Instant::now();
        let mut seen = ScreenSeen::default();
        let mut images = question.images.clone();
        let step = if images.is_empty() { self.vision.decide(&question.text) } else { Step::NoCapture };
        if step == Step::Capture {
            images.extend(self.capture(session, &mut seen, sink));
            if cancel.is_cancelled() {
                return Err(AskError::Cancelled);
            }
        }
        let had_image = !images.is_empty();
        let kind = self.screen_prompt(had_image);
        let setup = AskSetup { system: self.system_for(kind), ..self.setup.clone() };
        // Only where the prompt taught the model the marker can the
        // reply be one.
        let gated = kind == ScreenPrompt::OnRequest;
        let text = match &seen.for_model {
            Some(why) => prompt::user_message_with_note(&question.text, &prompt::no_screenshot_note(why)),
            None => prompt::user_message(&question.text),
        };
        let first = self.turn(backend, &setup, AskInput { text, images }, sink, cancel, gated);
        let mut turns = 1;
        let result = first.and_then(|(reply, wants_screen)| {
            if !wants_screen {
                return Ok(reply);
            }
            // The model cannot answer without the screen. Capture it
            // now, unless that was already tried for this ask.
            let follow_up = if had_image {
                AskInput::text(prompt::SCREEN_ALREADY_SENT)
            } else if let Some(why) = &seen.for_model {
                AskInput::text(prompt::no_screen_follow_up(why))
            } else {
                match self.capture(session, &mut seen, sink) {
                    Some(image) => AskInput { text: prompt::SCREEN_FOLLOWS.to_string(), images: vec![image] },
                    None => AskInput::text(prompt::no_screen_follow_up(seen.for_model.as_deref().unwrap_or_default())),
                }
            };
            if cancel.is_cancelled() {
                return Err(AskError::Cancelled);
            }
            turns = 2;
            let before = started.elapsed();
            let (second, again) = self.turn(backend, &setup, follow_up, sink, cancel, true)?;
            if again {
                // Asked for the screen twice: there is no answer.
                return Err(AskError::Backend(NO_ANSWER.to_string()));
            }
            Ok(both_turns(reply, second, before, started.elapsed()))
        });
        // Lengths only, never the text: verbose logs end up in bug
        // reports.
        crate::verbose_log!(
            "[ow.ask] backend={} ok={} chars_in={} chars_out={} first_text_ms={:?} ms={:?} turns={} screen={}",
            backend.id(),
            result.is_ok(),
            question.text.trim().chars().count(),
            result.as_ref().map_or(0, |r| r.text.chars().count()),
            result.as_ref().ok().and_then(|r| r.first_text_ms),
            result.as_ref().ok().map(|r| r.elapsed_ms),
            turns,
            seen.state.as_str()
        );
        let mut reply = result?;
        if reply.text.is_empty() {
            return Err(AskError::Backend(NO_ANSWER.to_string()));
        }
        if reply.notice.is_empty() {
            reply.notice = self.notice.clone();
        }
        reply.screen = seen.state;
        reply.screen_notice = seen.notice;
        Ok(reply)
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
        // The proof that an ask is asking. Made here, used once.
        let token = match session {
            Some(id) => AskCaptureToken::for_session(id),
            None => AskCaptureToken::for_one_shot(),
        };
        let (image, state, notice, for_model) = match screen::take(&token, &self.vision.settings) {
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
    /// request that sent it. With `gated`, the marker is kept from the
    /// sink; the flag in the result says that the reply was the
    /// marker.
    fn turn(
        &self,
        backend: &mut dyn AskBackend,
        setup: &AskSetup,
        input: AskInput,
        sink: &mut dyn FnMut(AskEvent),
        cancel: &CancelFlag,
        gated: bool,
    ) -> Result<(AskReply, bool), AskError> {
        if !gated {
            let result = backend.send(setup, &input, sink, cancel, self.timeout);
            drop(input);
            return result.map(|reply| (reply, false));
        }
        let mut gate = screen::MarkerGate::new();
        let result = backend.send(
            setup,
            &input,
            &mut |event| {
                if let Some(event) = gate.pass(event) {
                    sink(event);
                }
            },
            cancel,
            self.timeout,
        );
        drop(input);
        let reply = result?;
        if gate.saw_marker() || screen::is_marker(&reply.text) {
            return Ok((reply, true));
        }
        if let Some(event) = gate.flush() {
            sink(event);
        }
        Ok((reply, false))
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
        let result = self.send(backend, question, sink, cancel);
        let outcome = Outcome {
            plan: self,
            backend_id: backend.id(),
            question: &question.text,
            result: &result,
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
        screen: ScreenState::None,
        screen_notice: String::new(),
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
            Self::Done => "done",
            Self::Error => "error",
            Self::Cancelled => "cancelled",
        }
    }

    /// Something is in flight that a cancel would stop.
    pub fn is_busy(self) -> bool {
        matches!(self, Self::Listening | Self::Thinking | Self::Answering)
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
    /// Goes up on every change, so a shell can emit only on change.
    pub version: u64,
}

struct Warm {
    plan: AskPlan,
    backend: Box<dyn AskBackend>,
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
        self.snap.id == id && matches!(self.snap.phase, AskPhase::Thinking | AskPhase::Answering)
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
    let plan = session_plan();
    let mut backend = plan.backend();
    backend.warm(&plan.setup);
    crate::verbose_log!(
        "[ow.ask] warm backend={} setup_ms={} web_search={} (own thread, recording already running)",
        backend.id(),
        started.elapsed().as_millis(),
        plan.setup.web_search
    );
    let mut slot = Some(Warm { plan, backend });
    read(|s| {
        if s.listening(id) {
            s.warm = slot.take();
        }
    });
    drop(slot);
}

/// The plan of the app's ask as the settings are right now, the
/// Vision settings included.
fn session_plan() -> AskPlan {
    AskPlan::new(&crate::settings::current_ask_settings(), &crate::settings::current_refine_settings())
        .with_vision(&crate::settings::current_vision_settings(), VisionChoice::Settings)
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
    let plan = session_plan();
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
                }
            }
        });
    };
    let result = plan.converse(Some(id), backend.as_mut(), &AskInput::text(question), &mut sink, &cancel);
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
            AskPhase::Thinking | AskPhase::Answering => {
                s.snap.searching = false;
                s.snap.elapsed_ms = elapsed_ms;
                s.sent = None;
                match &result {
                    Ok(reply) => {
                        s.snap.answer = reply.text.clone();
                        s.snap.notice = reply.notice.clone();
                        s.snap.first_text_ms = s.snap.first_text_ms.or(reply.first_text_ms);
                        s.snap.usage = reply.usage.clone();
                        s.snap.screen = reply.screen;
                        s.snap.screen_notice = reply.screen_notice.clone();
                        s.snap.phase = AskPhase::Done;
                        true
                    }
                    Err(AskError::Cancelled) => {
                        settle_screen(&mut s.snap);
                        s.snap.phase = AskPhase::Cancelled;
                        false
                    }
                    Err(e) => {
                        settle_screen(&mut s.snap);
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
    /// Write the diagnostics row and, with `history`, keep an
    /// accepted answer in the transcript history.
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
            self.at_ms,
            self.elapsed_ms,
        ));
        if let (true, true, Ok(reply)) = (history, self.accepted, self.result) {
            let screen = reply.screen == ScreenState::Included;
            keep_in_history(self.question, &reply.text, self.at_ms, self.spoken_ms, screen);
        }
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
fn run_record(
    status: &str,
    plan: &AskPlan,
    backend_id: &str,
    question: &str,
    result: &Result<AskReply, AskError>,
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
        first_text_ms: reply.and_then(|r| r.first_text_ms).map(|ms| ms as i64),
    }
}

/// With transcript history on, keep the finished ask: the question as
/// the entry's text, the answer under it ([`crate::history`]).
/// Cancelled and failed asks are not kept. `screen`: a screenshot
/// went out with the question, which the entry says (the image itself
/// is kept nowhere).
fn keep_in_history(question: &str, answer: &str, at_ms: i64, spoken_ms: i64, screen: bool) {
    let settings = crate::settings::current_history_settings();
    let Some(dir) = crate::history::current_dir() else {
        return;
    };
    if settings.enabled {
        let entry = crate::history::NewEntry {
            at_ms: if at_ms > 0 { at_ms } else { now_ms() },
            duration_ms: spoken_ms,
            text: question,
            original: None,
            app_id: "",
            app_name: "",
            mode: "",
            kind: crate::history::KIND_ASK,
            answer: Some(answer),
            screen,
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
/// of the answer stays on the snapshot). While `listening`, the ask
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
            AskPhase::Thinking | AskPhase::Answering => {
                s.cancel.cancel();
                s.snap.phase = AskPhase::Cancelled;
                s.snap.searching = false;
                settle_screen(&mut s.snap);
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

/// The current ask.
pub fn snapshot() -> AskSnapshot {
    read(|s| {
        let mut snap = s.snap.clone();
        if let Some(sent) = s.sent.filter(|_| matches!(snap.phase, AskPhase::Thinking | AskPhase::Answering)) {
            snap.elapsed_ms = sent.elapsed().as_millis() as u64;
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
                usage: Some(CallUsage { tokens_in: 900, tokens_cached: 800, tokens_out: 40, cost_usd: Some(0.002), plan: None }),
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
