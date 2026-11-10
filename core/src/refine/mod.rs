//! Refine: an optional language-model pass over the transcript, after
//! the rule pass and the dictionary ([`crate::transcript`]) and before
//! the text reaches the UI, the cursor and the clipboard (TASK-93).
//!
//! ```text
//! record --> transcribe (local) --> rules + dictionary --> REFINE --> insert
//!   |                                                        ^
//!   +-- warm_for_recording(): backend starts up meanwhile ---+
//! ```
//!
//! One interface, [`Refiner`]: text in, text out. Two backends today,
//! both of which send the text off the machine, so the whole step is
//! opt-in (off by default):
//!
//! - [`claude_cli`]: the user's own installed and signed-in Claude
//!   Code CLI, run headless. No login or key lives in this app.
//! - [`openai_compat`]: any OpenAI-compatible chat-completions
//!   endpoint. URL and model in `settings.json`, API key in the OS
//!   keychain ([`secrets`]).
//!
//! A dictation is mostly the message, but it can also carry
//! instructions spoken to Refine about that message ("scratch that",
//! "make that more formal", "write this in English"). Refine carries
//! those out; questions and tasks meant for someone else stay text.
//! See [`prompt`] for where that line is drawn.
//!
//! What Refine does with the text is the *mode* ([`crate::modes`],
//! TASK-96). The built-in Dictation mode is the cleanup described
//! above, held to the strict guard. A mode the user made ("Formal",
//! "Write in English") sends its own instructions, and optionally its
//! own model and effort, and rewrites on purpose: its answer only has
//! to be text of a sane length. [`apply_mode`] turns a mode into the
//! settings and the prompt of one run; every outcome says which mode
//! it ran in ([`RefineOutcome::mode`]).
//!
//! The rule that shapes everything here: **Refine may never cost the
//! user their dictation.** A slow, failing, missing or misbehaving
//! backend yields the unrefined transcript ([`RefineOutcome::text`]
//! equals [`RefineOutcome::raw`]) and a status that says why. The
//! unrefined transcript is always kept on the outcome.
//!
//! Entry points:
//!
//! - the dictation pipeline calls [`warm_for_recording`] when capture
//!   starts, [`run_for_dictation`] when the transcript is ready, and
//!   [`discard_warm`] when a recording is cancelled or empty;
//! - the CLI and the settings pane call [`refine_text`],
//!   [`refine_text_in_mode`], [`backend_status`] and
//!   [`test_connection`] with explicit settings.

use std::fmt;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::modes::{ModeChoice, ModeReason};
use crate::settings::{
    BUILTIN_MODE_ID, BUILTIN_MODE_NAME, RefineBackendKind, RefineEffort, RefineSettings,
};

pub mod claude_cli;
pub mod eval;
pub mod guard;
pub mod history;
pub mod openai_compat;
pub mod prompt;
pub mod secrets;

pub use prompt::{DEFAULT_INSTRUCTIONS, Prompt};

/// Why a backend did not return text.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RefineError {
    /// The backend cannot run at all: CLI not installed or not signed
    /// in, endpoint not configured. The message says what to do.
    Unavailable(String),
    /// No answer within the timeout.
    Timeout,
    /// The backend ran and failed (HTTP error, process error, rate
    /// limit, malformed answer).
    Backend(String),
}

impl fmt::Display for RefineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(m) | Self::Backend(m) => f.write_str(m),
            Self::Timeout => f.write_str("no answer in time"),
        }
    }
}

impl std::error::Error for RefineError {}

/// What one model call used, as the backend reported it. For
/// diagnostics: how heavy is a dictation on tokens and on the plan.
#[derive(Serialize, Clone, Debug, PartialEq, Default)]
pub struct CallUsage {
    /// All input tokens, cached ones included.
    pub tokens_in: u64,
    /// The part of `tokens_in` read from the prompt cache (cheap).
    pub tokens_cached: u64,
    pub tokens_out: u64,
    /// What the call would cost at API list prices, when the backend
    /// says. On a subscription nothing is billed per call; this is a
    /// yardstick for size, not a charge.
    pub cost_usd: Option<f64>,
    /// Claude CLI on a subscription: how full the plan's usage
    /// windows are after this call.
    pub plan: Option<PlanWindows>,
}

/// The subscription's usage windows, as the Claude CLI reports them
/// with each call. They cover *all* use of the plan (Claude Code,
/// chat, Refine), not just Refine.
#[derive(Serialize, Clone, Debug, PartialEq, Default)]
pub struct PlanWindows {
    /// Share of the 5-hour window used, 0.0 to 1.0.
    pub five_hour_used: Option<f64>,
    /// Unix epoch seconds.
    pub five_hour_resets_at: Option<i64>,
    /// Share of the 7-day window used, 0.0 to 1.0.
    pub seven_day_used: Option<f64>,
    pub seven_day_resets_at: Option<i64>,
}

/// A Refine backend. Text in, text out.
///
/// Dropping a refiner must release whatever [`Refiner::warm`]
/// started (kill a child process, close a connection).
pub trait Refiner: Send {
    /// Stable id for logs and the UI, e.g. `"claude-cli"`.
    fn id(&self) -> &'static str;

    /// Start anything that can overlap the recording, so the later
    /// [`Refiner::refine`] call is fast. Must not send the transcript
    /// anywhere (there is none yet) and must not block. Default: no-op.
    fn warm(&mut self, _prompt: &Prompt) {}

    /// After a [`Refiner::refine`] call: how long before that request
    /// the backend had been started by [`Refiner::warm`], or `None`
    /// when the request ran cold. Diagnostics only.
    fn warm_lead(&self) -> Option<Duration> {
        None
    }

    /// After a [`Refiner::refine`] call that got an answer: tokens,
    /// list-price cost and plan windows, as far as the backend
    /// reports them. Diagnostics only.
    fn last_usage(&self) -> Option<CallUsage> {
        None
    }

    /// Send `transcript` with `prompt` and return the model's text.
    /// Must return within roughly `timeout`.
    fn refine(
        &mut self,
        prompt: &Prompt,
        transcript: &str,
        timeout: Duration,
    ) -> Result<String, RefineError>;
}

/// How a Refine run ended.
#[non_exhaustive]
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RefineStatus {
    /// Refine is switched off. Nothing was sent anywhere.
    Disabled,
    /// The model's text replaced the transcript.
    Refined,
    /// Too few words; no model call was made.
    SkippedShort,
    /// No answer in time. Unrefined transcript used.
    Timeout,
    /// Backend missing or not set up. Unrefined transcript used.
    Unavailable,
    /// Backend error. Unrefined transcript used.
    Failed,
    /// The model answered, but with something that is not a cleanup
    /// of what was said (see [`guard`]). Unrefined transcript used.
    Rejected,
}

impl RefineStatus {
    /// Stable id, same as the serialized form.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Refined => "refined",
            Self::SkippedShort => "skipped-short",
            Self::Timeout => "timeout",
            Self::Unavailable => "unavailable",
            Self::Failed => "failed",
            Self::Rejected => "rejected",
        }
    }

    /// True when a backend was asked and its answer was not used.
    pub fn is_fallback(self) -> bool {
        matches!(
            self,
            Self::Timeout | Self::Unavailable | Self::Failed | Self::Rejected
        )
    }
}

/// The mode a Refine run was made in, as it is recorded with the
/// outcome. The default is the built-in mode, selected.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct ModeUse {
    /// Mode id; [`BUILTIN_MODE_ID`] for the built-in mode.
    pub id: String,
    /// Mode name, as shown to a person.
    pub name: String,
    /// Why this mode ran: `selected` or `app-default`
    /// ([`ModeReason`]).
    pub reason: String,
    /// The app whose default mode this is; empty when the mode is
    /// simply the selected one.
    pub app_name: String,
}

impl Default for ModeUse {
    fn default() -> Self {
        Self {
            id: BUILTIN_MODE_ID.to_string(),
            name: BUILTIN_MODE_NAME.to_string(),
            reason: ModeReason::Selected.as_str().to_string(),
            app_name: String::new(),
        }
    }
}

impl ModeUse {
    /// What to record for a resolved mode.
    pub fn from_choice(choice: &ModeChoice) -> Self {
        Self {
            id: choice.mode.id.clone(),
            name: choice.mode.name.clone(),
            reason: choice.reason.as_str().to_string(),
            app_name: choice.app_name.clone(),
        }
    }

    /// The built-in Dictation mode.
    pub fn is_builtin(&self) -> bool {
        self.id == BUILTIN_MODE_ID
    }
}

/// Result of one Refine run.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct RefineOutcome {
    /// The text to insert. The refined text when
    /// `status == Refined`, otherwise exactly `raw`.
    pub text: String,
    /// The transcript as it came in (after rules and dictionary).
    pub raw: String,
    pub status: RefineStatus,
    /// Plain-language reason when the transcript was not refined;
    /// empty for `Refined` and `Disabled`.
    pub detail: String,
    /// The instruction the speaker gave Refine inside the dictation
    /// and Refine carried out ("make that more formal"), in their
    /// words. Empty when the dictation was only cleaned up.
    pub instruction: String,
    /// Backend id, empty when no backend was involved.
    pub backend: String,
    /// Model and effort the backend was asked to use.
    pub model: String,
    pub effort: String,
    /// Wall time spent in Refine, 0 when nothing was called.
    pub elapsed_ms: u64,
    /// When the backend had been started ahead of the request (at
    /// record start): how many ms ahead. `None` = it started cold.
    pub warm_lead_ms: Option<u64>,
    /// Tokens, list-price cost and plan windows of the call, when the
    /// backend answered and reported them.
    pub usage: Option<CallUsage>,
    /// The mode the run was made in. [`refine_with`] knows only the
    /// prompt, so it leaves the built-in mode here; the callers that
    /// resolved a mode ([`ModeRun::refine`]) fill it in.
    pub mode: ModeUse,
}

impl RefineOutcome {
    fn passthrough(raw: &str, status: RefineStatus, detail: impl Into<String>) -> Self {
        Self {
            text: raw.to_string(),
            raw: raw.to_string(),
            status,
            detail: detail.into(),
            instruction: String::new(),
            backend: String::new(),
            model: String::new(),
            effort: String::new(),
            elapsed_ms: 0,
            warm_lead_ms: None,
            usage: None,
            mode: ModeUse::default(),
        }
    }

    /// Outcome for "Refine is off": the transcript, untouched.
    pub fn disabled(raw: &str) -> Self {
        Self::passthrough(raw, RefineStatus::Disabled, "")
    }
}

// --- building blocks ----------------------------------------------

/// The backend selected in `settings`. Cheap: nothing is started
/// until [`Refiner::warm`] or [`Refiner::refine`].
pub fn backend_for(settings: &RefineSettings) -> Box<dyn Refiner> {
    #[cfg(test)]
    if let Some(make) = test_support::factory() {
        return make(settings);
    }
    match settings.backend {
        RefineBackendKind::ClaudeCli => {
            Box::new(claude_cli::ClaudeCli::new(settings.claude_cli.clone()))
        }
        RefineBackendKind::OpenaiCompatible => Box::new(openai_compat::OpenAiCompat::new(
            settings.openai.clone(),
            secrets::api_key().unwrap_or(None),
        )),
    }
}

/// The prompt for `settings`: guard rules, the user's instructions
/// and the current dictionary terms.
pub fn compose_prompt(settings: &RefineSettings) -> Prompt {
    let matcher = crate::dictionary::current_matcher();
    Prompt::compose(&settings.instructions, matcher.prompt_terms())
}

/// Run `text` through `backend` and apply every safety net: the
/// short-utterance skip, the timeout, the output guard, the fallback
/// to the unrefined text. Never fails; read the outcome's status.
///
/// A rewrite prompt ([`Prompt::is_rewrite`], a custom mode) changes
/// two of the nets. The short-utterance skip does not apply: two
/// words still have to be translated by a "write in English" mode.
/// And the answer is held to the lenient guard
/// ([`guard::check_instructed`]) only, since the strict one would
/// reject any rewrite.
///
/// Ignores `settings.enabled`: callers that got here decided to
/// refine (the pipeline checks the switch, the CLI and the connection
/// test run on demand).
pub fn refine_with(
    backend: &mut dyn Refiner,
    prompt: &Prompt,
    settings: &RefineSettings,
    text: &str,
) -> RefineOutcome {
    let input = text.trim();
    let words = guard::word_count(input);
    let rewrite = prompt.is_rewrite();
    if input.is_empty() || (!rewrite && words < settings.min_words as usize) {
        let detail = if rewrite {
            "no text: not sent".to_string()
        } else {
            format!(
                "{words} word{}, fewer than {}: not sent",
                if words == 1 { "" } else { "s" },
                settings.min_words
            )
        };
        return RefineOutcome::passthrough(text, RefineStatus::SkippedShort, detail);
    }

    let started = Instant::now();
    let timeout = Duration::from_millis(settings.timeout_ms);
    let result = backend.refine(prompt, input, timeout);
    let elapsed_ms = started.elapsed().as_millis() as u64;
    let backend_id = backend.id().to_string();
    let model = settings.active_model().to_string();
    let effort = settings.active_effort().to_string();
    let warm_lead_ms = backend.warm_lead().map(|d| d.as_millis() as u64);
    // Tokens were spent whether or not the answer gets used.
    let usage = if result.is_ok() { backend.last_usage() } else { None };

    let fallback = |status: RefineStatus, detail: String| RefineOutcome {
        text: text.to_string(),
        raw: text.to_string(),
        status,
        detail,
        instruction: String::new(),
        backend: backend_id.clone(),
        model: model.clone(),
        effort: effort.clone(),
        elapsed_ms,
        warm_lead_ms,
        usage: usage.clone(),
        mode: ModeUse::default(),
    };

    let outcome = match result {
        Ok(answer) => {
            // In the cleanup prompt a rewrite is only accepted when
            // the model names the spoken instruction that asked for
            // it. Without one, the text must still be what was said
            // (strict guard). A rewrite prompt asked for a rewrite.
            let (instruction, body) = prompt::split_instruction(&answer);
            let cleaned = prompt::clean_model_output(body, input);
            let verdict = if rewrite || instruction.is_some() {
                guard::check_instructed(input, &cleaned)
            } else {
                guard::check(input, &cleaned, settings.drift_guard)
            };
            match verdict {
                Ok(()) => RefineOutcome {
                    text: crate::transcript::finalize_refined(&cleaned),
                    raw: text.to_string(),
                    status: RefineStatus::Refined,
                    detail: String::new(),
                    instruction: instruction.unwrap_or_default(),
                    backend: backend_id.clone(),
                    model: model.clone(),
                    effort: effort.clone(),
                    elapsed_ms,
                    warm_lead_ms,
                    usage: usage.clone(),
                    mode: ModeUse::default(),
                },
                Err(reason) => fallback(RefineStatus::Rejected, reason),
            }
        }
        Err(RefineError::Timeout) => fallback(
            RefineStatus::Timeout,
            format!(
                "no answer within {:.1} s",
                settings.timeout_ms as f64 / 1000.0
            ),
        ),
        Err(RefineError::Unavailable(m)) => fallback(RefineStatus::Unavailable, m),
        Err(RefineError::Backend(m)) => fallback(RefineStatus::Failed, m),
    };

    // Lengths only, never the text: verbose logs end up in bug reports.
    crate::verbose_log!(
        "[ow.refine] backend={} status={} ms={} chars_in={} chars_out={} instruction={}",
        outcome.backend,
        outcome.status.as_str(),
        outcome.elapsed_ms,
        input.chars().count(),
        outcome.text.trim().chars().count(),
        !outcome.instruction.is_empty()
    );
    outcome
}

/// Refine `text` with a fresh backend built from `settings`, in the
/// built-in Dictation mode. For the connection test and one-off
/// calls; the dictation pipeline uses [`run_for_dictation`], which
/// resolves the mode and can reuse a warmed backend.
pub fn refine_text(settings: &RefineSettings, text: &str) -> RefineOutcome {
    let prompt = compose_prompt(settings);
    let mut backend = backend_for(settings);
    refine_with(backend.as_mut(), &prompt, settings, text)
}

// --- modes ----------------------------------------------------------

/// One Refine run in a mode, ready to go: the settings with the mode
/// applied, the prompt, and what to record. Made by [`apply_mode`].
///
/// The pipeline and the CLI go through the same type, so `openwhisper
/// refine run --mode` does exactly what a dictation in that mode does.
#[derive(Clone, Debug)]
pub struct ModeRun {
    /// The Refine settings with the mode's instructions, model and
    /// effort in place. Build the backend from these.
    pub settings: RefineSettings,
    /// The cleanup prompt for the built-in mode (and for a custom
    /// mode without instructions), the mode prompt otherwise.
    pub prompt: Prompt,
    /// Recorded on the outcome.
    pub mode: ModeUse,
}

impl ModeRun {
    /// Like [`apply_mode`], with the dictionary terms given instead
    /// of read from the user's dictionary. For the eval set and tests.
    pub fn with_terms(settings: &RefineSettings, choice: &ModeChoice, terms: &[String]) -> Self {
        let mode = &choice.mode;
        let mut settings = settings.clone();
        if !mode.builtin {
            if let Some(model) = mode.model.as_deref().map(str::trim).filter(|m| !m.is_empty()) {
                match settings.backend {
                    RefineBackendKind::OpenaiCompatible => settings.openai.model = model.to_string(),
                    _ => settings.claude_cli.model = model.to_string(),
                }
            }
            if let Some(effort) = mode.effort {
                match settings.backend {
                    RefineBackendKind::OpenaiCompatible => settings.openai.effort = Some(effort),
                    // The Claude CLI has no "minimal" level.
                    _ if effort == RefineEffort::Minimal => settings.claude_cli.effort = RefineEffort::Low,
                    _ => settings.claude_cli.effort = effort,
                }
            }
        }
        let prompt = if mode.rewrites() {
            settings.instructions = mode.instructions.trim().to_string();
            Prompt::compose_mode(&mode.name, &mode.instructions, terms)
        } else {
            Prompt::compose(&settings.instructions, terms)
        };
        Self {
            settings,
            prompt,
            mode: ModeUse::from_choice(choice),
        }
    }

    /// Run `text` through `backend` in this mode. Same safety nets as
    /// [`refine_with`]; the outcome names the mode.
    pub fn refine(&self, backend: &mut dyn Refiner, text: &str) -> RefineOutcome {
        let mut outcome = refine_with(backend, &self.prompt, &self.settings, text);
        outcome.mode = self.mode.clone();
        outcome
    }
}

/// Apply a resolved mode ([`crate::modes::resolve`]) to the Refine
/// settings and compose its prompt, with the current dictionary terms.
///
/// - The built-in mode changes nothing: the Refine instructions, the
///   cleanup prompt, the strict guard.
/// - A custom mode puts its model and effort (when it has them) on
///   the selected backend and sends the mode prompt with its
///   instructions. Without instructions it keeps the cleanup prompt
///   and only the model or effort differ.
pub fn apply_mode(settings: &RefineSettings, choice: &ModeChoice) -> ModeRun {
    let matcher = crate::dictionary::current_matcher();
    ModeRun::with_terms(settings, choice, matcher.prompt_terms())
}

/// Refine `text` in a mode with a fresh backend built from `settings`.
/// What `openwhisper refine run --mode` calls, and what the dictation
/// pipeline calls when no backend was warmed.
pub fn refine_text_in_mode(settings: &RefineSettings, choice: &ModeChoice, text: &str) -> RefineOutcome {
    let run = apply_mode(settings, choice);
    let mut backend = backend_for(&run.settings);
    run.refine(backend.as_mut(), text)
}

/// Id and name of the frontmost app, for the per-app default mode.
/// `None` when it is not known or is OpenWhisper itself: its own
/// windows never match an app default.
#[cfg(all(feature = "focus", not(test)))]
fn frontmost_app() -> Option<(String, String)> {
    let focus = crate::focus::current();
    if focus.is_self || focus.app_id.is_empty() {
        return None;
    }
    Some((focus.app_id, focus.app_name))
}

#[cfg(all(not(feature = "focus"), not(test)))]
fn frontmost_app() -> Option<(String, String)> {
    None
}

/// Unit tests never ask the OS which app is in front: the answer
/// would be whatever the developer has open.
#[cfg(test)]
fn frontmost_app() -> Option<(String, String)> {
    test_support::frontmost_app()
}

/// The mode for the dictation in flight: the frontmost app's default
/// when it has one, else the selected mode. The app is only read when
/// at least one app default exists, so users without any pay nothing.
fn resolve_for_dictation(settings: &RefineSettings) -> ModeChoice {
    let modes = crate::settings::current_modes_settings();
    let app = if modes.app_defaults.is_empty() { None } else { frontmost_app() };
    crate::modes::resolve(
        &modes,
        settings,
        app.as_ref().map(|(id, name)| (id.as_str(), name.as_str())),
    )
}

// --- dictation pipeline hooks -------------------------------------

struct WarmSlot {
    backend: Box<dyn Refiner>,
    /// Settings, prompt and mode as they were when the recording
    /// started. The run uses these, so the warmed and the cold path
    /// record the mode the same way.
    run: ModeRun,
}

/// The backend warmed for the recording in flight, if any. One slot:
/// there is one dictation at a time.
static WARM: Mutex<Option<WarmSlot>> = Mutex::new(None);

/// Bumped every time the slot is emptied. Warming runs on its own
/// thread; if the recording it was started for is already over by the
/// time the backend is up (a very short dictation, a quick cancel),
/// the epoch has moved and the late backend is dropped instead of
/// being parked in the slot.
static EPOCH: AtomicU64 = AtomicU64::new(0);

fn take_warm() -> Option<WarmSlot> {
    let mut slot = WARM.lock().ok()?;
    EPOCH.fetch_add(1, Ordering::SeqCst);
    slot.take()
}

/// Called when capture starts. With Refine on, works out the mode,
/// builds the backend and lets it start up while the user talks (the
/// Claude CLI takes over a second to boot; warmed, the answer comes
/// back in well under one). With Refine off this does nothing at all:
/// the switch is checked before the mode is even resolved.
///
/// Reads the cached settings ([`crate::settings::current_refine_settings`],
/// [`crate::settings::current_modes_settings`]) and, when an app has a
/// default mode, the frontmost app. May block briefly (dictionary
/// read, keychain read, process spawn), so call it off the hotkey
/// thread.
pub fn warm_for_recording() {
    let settings = crate::settings::current_refine_settings();
    // Whatever was left from an earlier recording goes first.
    drop(take_warm());
    if !settings.enabled {
        return;
    }
    let epoch = EPOCH.load(Ordering::SeqCst);
    let started = Instant::now();
    let choice = resolve_for_dictation(&settings);
    let run = apply_mode(&settings, &choice);
    let mut backend = backend_for(&run.settings);
    backend.warm(&run.prompt);
    crate::verbose_log!(
        "[ow.refine] warm backend={} setup_ms={} custom_mode={} reason={} (own thread, recording already running)",
        backend.id(),
        started.elapsed().as_millis(),
        !run.mode.is_builtin(),
        run.mode.reason
    );
    if let Ok(mut slot) = WARM.lock() {
        if EPOCH.load(Ordering::SeqCst) == epoch {
            *slot = Some(WarmSlot { backend, run });
        }
        // Otherwise the recording already ended: `backend` drops here.
    }
}

/// Drop the warmed backend (kills a warmed CLI process). Called when
/// a recording is cancelled, came back empty, or failed to transcribe.
pub fn discard_warm() {
    drop(take_warm());
}

/// Refine the transcript of the dictation that just finished. Uses
/// the backend warmed by [`warm_for_recording`] when there is one,
/// with the settings and the mode that were current at that moment;
/// otherwise resolves the mode now and builds a cold backend from the
/// current settings.
///
/// With Refine off, returns [`RefineOutcome::disabled`] and starts
/// nothing, whatever mode is selected.
pub fn run_for_dictation(text: &str) -> RefineOutcome {
    let outcome = match take_warm() {
        Some(mut slot) => slot.run.refine(slot.backend.as_mut(), text),
        None => {
            let settings = crate::settings::current_refine_settings();
            if !settings.enabled {
                return RefineOutcome::disabled(text);
            }
            let choice = resolve_for_dictation(&settings);
            refine_text_in_mode(&settings, &choice, text)
        }
    };
    history::record_outcome(history::SOURCE_DICTATION, &outcome);
    outcome
}

/// True when the pipeline should show a "refining" status for this
/// transcript: Refine is on and the text will be sent. In the
/// built-in mode that takes [`RefineSettings::min_words`] words; a
/// mode that rewrites sends any text.
pub fn will_call_backend(text: &str) -> bool {
    let warmed = match WARM.lock() {
        Ok(g) => g.as_ref().map(|s| (s.run.settings.clone(), s.run.prompt.is_rewrite())),
        Err(_) => None,
    };
    let (settings, rewrite) = match warmed {
        Some(found) => found,
        None => {
            let settings = crate::settings::current_refine_settings();
            if !settings.enabled {
                return false;
            }
            let rewrite = resolve_for_dictation(&settings).mode.rewrites();
            (settings, rewrite)
        }
    };
    let words = guard::word_count(text);
    settings.enabled && if rewrite { words > 0 } else { words >= settings.min_words as usize }
}

// --- status and connection test -----------------------------------

/// Whether a backend can be used right now.
#[non_exhaustive]
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BackendState {
    /// Ready to use.
    Ready,
    /// Claude CLI: no `claude` binary found.
    NotInstalled,
    /// Claude CLI: found, but not signed in.
    NotSignedIn,
    /// OpenAI-compatible: URL or model missing.
    NotConfigured,
    /// The check itself failed (the CLI did not answer).
    Unknown,
}

/// What the settings pane and `openwhisper refine status` show for a
/// backend.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct BackendStatus {
    /// Backend id (`"claude-cli"`, `"openai-compatible"`).
    pub backend: String,
    pub state: BackendState,
    /// `state == Ready`.
    pub ready: bool,
    /// One plain sentence: what is wrong and what to do, or that all
    /// is well.
    pub message: String,
    /// Where dictated text is sent when this backend is used. Shown
    /// verbatim next to the Refine switch.
    pub destination: String,
    /// Claude CLI: resolved binary path.
    pub path: Option<String>,
    /// Claude CLI: `claude --version` output.
    pub version: Option<String>,
    /// Claude CLI: how it is signed in (`"claude.ai"` = subscription,
    /// or an API key / cloud provider).
    pub auth_method: Option<String>,
    /// Claude CLI: subscription type as the CLI reports it.
    pub plan: Option<String>,
    /// OpenAI-compatible: an API key is stored (keychain or env).
    pub has_api_key: bool,
}

/// Check whether the backend selected in `settings` can run. For the
/// Claude CLI this runs `claude auth status` (about 0.2 s); for the
/// OpenAI-compatible backend it only looks at the configuration and
/// makes no network call (use [`test_connection`] for that).
pub fn backend_status(settings: &RefineSettings) -> BackendStatus {
    match settings.backend {
        RefineBackendKind::ClaudeCli => claude_cli::status(&settings.claude_cli),
        RefineBackendKind::OpenaiCompatible => openai_compat::status(&settings.openai),
    }
}

/// The sentence both shells show while testing: what goes through the
/// model in a connection test.
pub const TEST_SENTENCE: &str =
    "so um this is a test of the refine step uh it should come back with punctuation";

/// Result of [`test_connection`].
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct ConnectionTest {
    /// The model answered with a usable cleanup.
    pub ok: bool,
    /// Backend id.
    pub backend: String,
    /// What the model returned for [`TEST_SENTENCE`] (empty on failure).
    pub output: String,
    /// Why it failed (empty on success).
    pub message: String,
    pub elapsed_ms: u64,
}

/// Send [`TEST_SENTENCE`] through the backend in `settings` (cold, so
/// the time shown is the worst case) and report what came back.
/// Works whether or not Refine is enabled.
pub fn test_connection(settings: &RefineSettings) -> ConnectionTest {
    // The test sentence must always be sent, whatever `min_words` is.
    let mut settings = settings.clone();
    settings.min_words = 0;
    let outcome = refine_text(&settings, TEST_SENTENCE);
    history::record_outcome(history::SOURCE_TEST, &outcome);
    let ok = outcome.status == RefineStatus::Refined;
    ConnectionTest {
        ok,
        backend: if outcome.backend.is_empty() {
            settings.backend.as_str().to_string()
        } else {
            outcome.backend
        },
        output: if ok {
            outcome.text.trim().to_string()
        } else {
            String::new()
        },
        message: if ok { String::new() } else { outcome.detail },
        elapsed_ms: outcome.elapsed_ms,
    }
}

// --- test support --------------------------------------------------

/// Stub backend and factory override for unit tests in this crate.
#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use std::sync::{Arc, MutexGuard};

    type Factory = Arc<dyn Fn(&RefineSettings) -> Box<dyn Refiner> + Send + Sync>;

    static FACTORY: Mutex<Option<Factory>> = Mutex::new(None);
    /// What tests pretend the frontmost app is (id, name).
    static FRONTMOST: Mutex<Option<(String, String)>> = Mutex::new(None);

    pub(crate) fn frontmost_app() -> Option<(String, String)> {
        FRONTMOST.lock().ok().and_then(|g| g.clone())
    }

    /// Pretend this app is in front, until the [`Scope`] ends.
    pub(crate) fn set_frontmost_app(app: Option<(&str, &str)>) {
        if let Ok(mut g) = FRONTMOST.lock() {
            *g = app.map(|(id, name)| (id.to_string(), name.to_string()));
        }
    }

    pub(crate) fn factory() -> Option<Factory> {
        FACTORY.lock().ok().and_then(|g| g.clone())
    }

    /// Holds the global test lock and resets shared state on drop.
    pub(crate) struct Scope {
        _guard: MutexGuard<'static, ()>,
    }

    impl Drop for Scope {
        fn drop(&mut self) {
            super::discard_warm();
            if let Ok(mut g) = FACTORY.lock() {
                *g = None;
            }
            set_frontmost_app(None);
            crate::settings::set_refine_settings_cache(RefineSettings::default());
            crate::settings::set_modes_settings_cache(crate::settings::ModesSettings::default());
        }
    }

    /// Install `make` as the backend factory and `settings` as the
    /// current settings for the lifetime of the returned scope. Modes
    /// start at the defaults (only the built-in mode) and no app is
    /// in front.
    ///
    /// Holds the lock that serializes tests touching the process-wide
    /// warm slot, the settings caches or the factory override.
    pub(crate) fn scope(
        settings: RefineSettings,
        make: impl Fn(&RefineSettings) -> Box<dyn Refiner> + Send + Sync + 'static,
    ) -> Scope {
        let guard = crate::settings::CACHE_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        super::discard_warm();
        if let Ok(mut g) = FACTORY.lock() {
            *g = Some(Arc::new(make));
        }
        set_frontmost_app(None);
        crate::settings::set_refine_settings_cache(settings);
        crate::settings::set_modes_settings_cache(crate::settings::ModesSettings::default());
        Scope { _guard: guard }
    }

    /// What a [`Stub`] saw and did.
    #[derive(Default, Debug)]
    pub(crate) struct Calls {
        pub warmed: usize,
        pub refined: Vec<String>,
        pub dropped: usize,
        pub last_system: String,
        pub last_user: String,
    }

    /// Backend that answers from a closure and records its calls.
    pub(crate) struct Stub {
        pub calls: Arc<Mutex<Calls>>,
        pub answer: Arc<dyn Fn(&str) -> Result<String, RefineError> + Send + Sync>,
    }

    impl Stub {
        pub(crate) fn new(
            calls: Arc<Mutex<Calls>>,
            answer: impl Fn(&str) -> Result<String, RefineError> + Send + Sync + 'static,
        ) -> Self {
            Self {
                calls,
                answer: Arc::new(answer),
            }
        }
    }

    impl Refiner for Stub {
        fn id(&self) -> &'static str {
            "stub"
        }
        fn warm(&mut self, _prompt: &Prompt) {
            self.calls.lock().unwrap().warmed += 1;
        }
        fn refine(
            &mut self,
            prompt: &Prompt,
            transcript: &str,
            _timeout: Duration,
        ) -> Result<String, RefineError> {
            let mut c = self.calls.lock().unwrap();
            c.refined.push(transcript.to_string());
            c.last_system = prompt.system().to_string();
            c.last_user = prompt.user_message(transcript);
            drop(c);
            (self.answer)(transcript)
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
mod tests {
    use super::test_support::{Calls, Stub, scope};
    use super::*;
    use std::sync::Arc;

    fn on() -> RefineSettings {
        RefineSettings {
            enabled: true,
            ..RefineSettings::default()
        }
    }

    fn stub_with(
        answer: impl Fn(&str) -> Result<String, RefineError> + Send + Sync + 'static,
    ) -> (Arc<Mutex<Calls>>, Stub) {
        let calls = Arc::new(Mutex::new(Calls::default()));
        let stub = Stub::new(calls.clone(), answer);
        (calls, stub)
    }

    const SAID: &str = "so i think we should move the meeting with jeppe to thursday ";

    #[test]
    fn refined_text_replaces_the_transcript_and_raw_is_kept() {
        let (calls, mut stub) =
            stub_with(|_| Ok("I think we should move the meeting with Jeppe to Thursday.".into()));
        let prompt = Prompt::compose("", &[]);
        let out = refine_with(&mut stub, &prompt, &on(), SAID);
        assert_eq!(out.status, RefineStatus::Refined);
        // Trailing space like the rule pass, for back-to-back pastes.
        assert_eq!(
            out.text,
            "I think we should move the meeting with Jeppe to Thursday. "
        );
        assert_eq!(out.raw, SAID);
        assert_eq!(out.backend, "stub");
        assert!(out.detail.is_empty());
        // The backend got the trimmed transcript, once.
        assert_eq!(calls.lock().unwrap().refined, vec![SAID.trim().to_string()]);
    }

    #[test]
    fn one_call_carries_instructions_terms_and_transcript_as_data() {
        let (calls, mut stub) = stub_with(|t| Ok(t.to_string()));
        let prompt = Prompt::compose(
            "Use British spelling.",
            &["PayProff".to_string(), "Emanoil".to_string()],
        );
        let said = "write an email to emanoil about the payproff release";
        let _ = refine_with(&mut stub, &prompt, &on(), said);
        let c = calls.lock().unwrap();
        assert_eq!(c.refined.len(), 1);
        assert!(c.last_system.contains("Use British spelling."));
        assert!(c.last_system.contains("PayProff; Emanoil"));
        assert!(!c.last_system.contains(said));
        assert_eq!(c.last_user, format!("<transcript>\n{said}\n</transcript>"));
    }

    #[test]
    fn short_utterances_never_reach_the_backend() {
        let (calls, mut stub) = stub_with(|_| Ok("should not be used".into()));
        let prompt = Prompt::compose("", &[]);
        let out = refine_with(&mut stub, &prompt, &on(), "okay thanks ");
        assert_eq!(out.status, RefineStatus::SkippedShort);
        assert_eq!(out.text, "okay thanks ");
        assert_eq!(out.elapsed_ms, 0);
        assert!(calls.lock().unwrap().refined.is_empty());
    }

    #[test]
    fn timeout_falls_back_to_the_unrefined_text() {
        let (_, mut stub) = stub_with(|_| Err(RefineError::Timeout));
        let out = refine_with(&mut stub, &Prompt::compose("", &[]), &on(), SAID);
        assert_eq!(out.status, RefineStatus::Timeout);
        assert_eq!(out.text, SAID);
        assert!(out.detail.contains("5.0 s"), "{}", out.detail);
    }

    #[test]
    fn missing_backend_falls_back_with_the_reason() {
        let (_, mut stub) =
            stub_with(|_| Err(RefineError::Unavailable("Claude Code is not installed.".into())));
        let out = refine_with(&mut stub, &Prompt::compose("", &[]), &on(), SAID);
        assert_eq!(out.status, RefineStatus::Unavailable);
        assert_eq!(out.text, SAID);
        assert_eq!(out.detail, "Claude Code is not installed.");
    }

    #[test]
    fn backend_error_falls_back() {
        let (_, mut stub) = stub_with(|_| Err(RefineError::Backend("HTTP 500".into())));
        let out = refine_with(&mut stub, &Prompt::compose("", &[]), &on(), SAID);
        assert_eq!(out.status, RefineStatus::Failed);
        assert_eq!(out.text, SAID);
        assert!(out.status.is_fallback());
    }

    #[test]
    fn a_model_that_obeys_the_dictation_is_rejected() {
        let said = "write an email to jeppe and tell him the invoice is ready ";
        let (_, mut stub) = stub_with(|_| {
            Ok("Subject: Invoice ready\n\nHi Jeppe,\n\nJust a quick note to let you know \
that the invoice is ready. Let me know if you have any questions.\n\nBest regards"
                .into())
        });
        let out = refine_with(&mut stub, &Prompt::compose("", &[]), &on(), said);
        assert_eq!(out.status, RefineStatus::Rejected);
        assert_eq!(out.text, said);
    }

    #[test]
    fn a_model_that_answers_a_question_is_rejected() {
        let said = "what is the capital of france and how many people live there ";
        let (_, mut stub) =
            stub_with(|_| Ok("Paris is the capital, with about two million inhabitants.".into()));
        let out = refine_with(&mut stub, &Prompt::compose("", &[]), &on(), said);
        assert_eq!(out.status, RefineStatus::Rejected);
        assert_eq!(out.text, said);
    }

    #[test]
    fn a_model_that_translates_danish_is_rejected() {
        let said = "jeg synes vi skal vente med at sende det ud til kunderne ";
        let (_, mut stub) =
            stub_with(|_| Ok("I think we should wait before sending it out to the customers.".into()));
        let out = refine_with(&mut stub, &Prompt::compose("", &[]), &on(), said);
        assert_eq!(out.status, RefineStatus::Rejected);
        assert_eq!(out.text, said);
    }

    #[test]
    fn a_declared_spoken_instruction_lets_the_rewrite_through() {
        let said = "hej emanoil vi rykker mødet til fredag fordi der er release torsdag \
skriv det på engelsk ";
        let (_, mut stub) = stub_with(|_| {
            Ok("[instruction: skriv det på engelsk]\nHi Emanoil, we're moving the meeting to \
Friday because there's a release on Thursday."
                .into())
        });
        let out = refine_with(&mut stub, &Prompt::compose("", &[]), &on(), said);
        assert_eq!(out.status, RefineStatus::Refined);
        assert_eq!(out.instruction, "skriv det på engelsk");
        // The marker line never reaches the cursor.
        assert_eq!(
            out.text,
            "Hi Emanoil, we're moving the meeting to Friday because there's a release on Thursday. "
        );
        assert_eq!(out.raw, said);
    }

    #[test]
    fn the_same_rewrite_without_a_declared_instruction_is_rejected() {
        let said = "hej emanoil vi rykker mødet til fredag fordi der er release torsdag ";
        let (_, mut stub) = stub_with(|_| {
            Ok("Hi Emanoil, we're moving the meeting to Friday because there's a release on Thursday.".into())
        });
        let out = refine_with(&mut stub, &Prompt::compose("", &[]), &on(), said);
        assert_eq!(out.status, RefineStatus::Rejected);
        assert_eq!(out.text, said);
        assert!(out.instruction.is_empty());
    }

    #[test]
    fn a_declared_instruction_does_not_excuse_an_essay() {
        let said = "tell jeppe the release is delayed make it more formal ";
        let (_, mut stub) = stub_with(|_| {
            Ok(format!(
                "[instruction: make it more formal]\n{}",
                "I regret to inform you that the release has been delayed. ".repeat(20)
            ))
        });
        let out = refine_with(&mut stub, &Prompt::compose("", &[]), &on(), said);
        assert_eq!(out.status, RefineStatus::Rejected);
        assert_eq!(out.text, said);
    }

    #[test]
    fn plain_cleanup_has_no_instruction() {
        let (_, mut stub) =
            stub_with(|_| Ok("I think we should move the meeting with Jeppe to Thursday.".into()));
        let out = refine_with(&mut stub, &Prompt::compose("", &[]), &on(), SAID);
        assert_eq!(out.status, RefineStatus::Refined);
        assert!(out.instruction.is_empty());
    }

    #[test]
    fn empty_answer_is_rejected() {
        let (_, mut stub) = stub_with(|_| Ok("   ".into()));
        let out = refine_with(&mut stub, &Prompt::compose("", &[]), &on(), SAID);
        assert_eq!(out.status, RefineStatus::Rejected);
        assert_eq!(out.text, SAID);
    }

    #[test]
    fn dictionary_is_applied_again_after_the_model() {
        // Built-in term: the model "normalized" the casing.
        let (_, mut stub) = stub_with(|_| Ok("I think Openwhisper is ready to ship today.".into()));
        let out = refine_with(
            &mut stub,
            &Prompt::compose("", &[]),
            &on(),
            "i think openwhisper is ready to ship today",
        );
        assert_eq!(out.status, RefineStatus::Refined);
        assert_eq!(out.text, "I think OpenWhisper is ready to ship today. ");
    }

    #[test]
    fn disabled_pipeline_starts_nothing() {
        let calls = Arc::new(Mutex::new(Calls::default()));
        let made = Arc::new(Mutex::new(0usize));
        let (c, m) = (calls.clone(), made.clone());
        let _scope = scope(RefineSettings::default(), move |_| {
            *m.lock().unwrap() += 1;
            Box::new(Stub::new(c.clone(), |t| Ok(t.to_string())))
        });
        warm_for_recording();
        assert!(!will_call_backend(SAID));
        let out = run_for_dictation(SAID);
        assert_eq!(out.status, RefineStatus::Disabled);
        assert_eq!(out.text, SAID);
        assert_eq!(*made.lock().unwrap(), 0, "no backend may be built when off");
        assert_eq!(calls.lock().unwrap().warmed, 0);
    }

    #[test]
    fn pipeline_warms_at_record_start_and_reuses_that_backend() {
        let calls = Arc::new(Mutex::new(Calls::default()));
        let made = Arc::new(Mutex::new(0usize));
        let (c, m) = (calls.clone(), made.clone());
        let _scope = scope(on(), move |_| {
            *m.lock().unwrap() += 1;
            Box::new(Stub::new(c.clone(), |_| {
                Ok("I think we should move the meeting with Jeppe to Thursday.".into())
            }))
        });
        warm_for_recording();
        assert_eq!(calls.lock().unwrap().warmed, 1);
        assert!(will_call_backend(SAID));
        let out = run_for_dictation(SAID);
        assert_eq!(out.status, RefineStatus::Refined);
        assert_eq!(*made.lock().unwrap(), 1, "the warmed backend is the one used");
        assert_eq!(calls.lock().unwrap().dropped, 1, "and it is released after");
    }

    #[test]
    fn cancel_discards_the_warm_backend() {
        let calls = Arc::new(Mutex::new(Calls::default()));
        let c = calls.clone();
        let _scope = scope(on(), move |_| {
            Box::new(Stub::new(c.clone(), |t| Ok(t.to_string())))
        });
        warm_for_recording();
        discard_warm();
        let c = calls.lock().unwrap();
        assert_eq!(c.warmed, 1);
        assert_eq!(c.dropped, 1);
        assert!(c.refined.is_empty());
    }

    #[test]
    fn warm_that_finishes_after_the_recording_ended_is_dropped() {
        let calls = Arc::new(Mutex::new(Calls::default()));
        let c = calls.clone();
        let _scope = scope(on(), move |_| {
            // Building the backend "takes a while": the recording is
            // cancelled before it is ready.
            discard_warm();
            Box::new(Stub::new(c.clone(), |t| Ok(t.to_string())))
        });
        warm_for_recording();
        assert!(WARM.lock().unwrap().is_none(), "late backend must not be parked");
        assert_eq!(calls.lock().unwrap().dropped, 1);
    }

    #[test]
    fn pipeline_without_warm_backend_runs_cold() {
        let calls = Arc::new(Mutex::new(Calls::default()));
        let c = calls.clone();
        let _scope = scope(on(), move |_| {
            Box::new(Stub::new(c.clone(), |_| {
                Ok("I think we should move the meeting with Jeppe to Thursday.".into())
            }))
        });
        let out = run_for_dictation(SAID);
        assert_eq!(out.status, RefineStatus::Refined);
        assert_eq!(calls.lock().unwrap().warmed, 0);
    }

    // --- modes ------------------------------------------------------

    const DANISH: &str = "hej anna jeg når ikke mødet i morgen kan vi rykke det til torsdag ";
    const ENGLISH: &str = "Hi Anna, I can't make the meeting tomorrow. Can we move it to Thursday?";

    /// Dictation plus "Formal" and "Write in English" (own model and
    /// effort), with `active` selected.
    fn modes_with(active: &str) -> crate::settings::ModesSettings {
        use crate::modes;
        let m = modes::add(
            &crate::settings::ModesSettings::default(),
            "Formal",
            "Rewrite what I say in a formal, polite tone.",
            None,
            None,
        )
        .unwrap();
        let m = modes::add(
            &m,
            "Write in English",
            "Write what I say in English.",
            Some("opus"),
            Some(RefineEffort::High),
        )
        .unwrap();
        modes::select(&m, active).unwrap()
    }

    fn choice(modes: &crate::settings::ModesSettings, settings: &RefineSettings) -> ModeChoice {
        crate::modes::resolve(modes, settings, None)
    }

    #[test]
    fn the_built_in_mode_changes_nothing() {
        let settings = RefineSettings {
            instructions: "Use British spelling.".into(),
            ..on()
        };
        let run = ModeRun::with_terms(&settings, &choice(&modes_with("dictation"), &settings), &[]);
        assert_eq!(run.settings, settings);
        assert_eq!(run.prompt, Prompt::compose("Use British spelling.", &[]));
        assert!(!run.prompt.is_rewrite());
        assert_eq!(run.mode, ModeUse::default());
        assert!(run.mode.is_builtin());
    }

    #[test]
    fn a_custom_mode_brings_its_prompt_model_and_effort() {
        let settings = on();
        let run = ModeRun::with_terms(
            &settings,
            &choice(&modes_with("write-in-english"), &settings),
            &["Anna".to_string()],
        );
        assert!(run.prompt.is_rewrite());
        assert!(run.prompt.system().contains("Write what I say in English."));
        assert!(run.prompt.system().contains("means one of them: Anna."));
        assert_eq!(run.settings.claude_cli.model, "opus");
        assert_eq!(run.settings.claude_cli.effort, RefineEffort::High);
        assert_eq!(run.settings.active_model(), "opus");
        assert_eq!(run.mode.name, "Write in English");
        assert_eq!(run.mode.reason, "selected");

        // A mode without a model or effort of its own keeps Refine's.
        let run = ModeRun::with_terms(&settings, &choice(&modes_with("formal"), &settings), &[]);
        assert_eq!(run.settings.claude_cli, settings.claude_cli);
        assert!(run.prompt.is_rewrite());

        // The model and effort land on whichever backend is selected,
        // and the Claude CLI never gets "minimal".
        let openai = RefineSettings {
            backend: RefineBackendKind::OpenaiCompatible,
            ..on()
        };
        let run = ModeRun::with_terms(&openai, &choice(&modes_with("write-in-english"), &openai), &[]);
        assert_eq!(run.settings.openai.model, "opus");
        assert_eq!(run.settings.openai.effort, Some(RefineEffort::High));
        assert_eq!(run.settings.claude_cli, openai.claude_cli);
        let minimal = crate::modes::edit(
            &modes_with("formal"),
            "formal",
            &crate::modes::ModeEdit {
                effort: Some(Some(RefineEffort::Minimal)),
                ..Default::default()
            },
        )
        .unwrap();
        let run = ModeRun::with_terms(&settings, &choice(&minimal, &settings), &[]);
        assert_eq!(run.settings.claude_cli.effort, RefineEffort::Low);
    }

    #[test]
    fn a_custom_mode_without_instructions_cleans_up_with_its_own_model() {
        let settings = on();
        let modes = crate::modes::add(
            &crate::settings::ModesSettings::default(),
            "Careful",
            "",
            Some("opus"),
            None,
        )
        .unwrap();
        let modes = crate::modes::select(&modes, "careful").unwrap();
        let run = ModeRun::with_terms(&settings, &choice(&modes, &settings), &[]);
        assert!(!run.prompt.is_rewrite());
        assert_eq!(run.prompt, Prompt::compose("", &[]));
        assert_eq!(run.settings.claude_cli.model, "opus");
        assert_eq!(run.mode.name, "Careful");
    }

    #[test]
    fn a_rewrite_passes_the_lenient_guard_where_the_strict_one_rejects_it() {
        // The same answer, once under the cleanup prompt and once
        // under a mode prompt.
        let (_, mut stub) = stub_with(|_| Ok(ENGLISH.into()));
        let strict = refine_with(&mut stub, &Prompt::compose("", &[]), &on(), DANISH);
        assert_eq!(strict.status, RefineStatus::Rejected);
        assert_eq!(strict.text, DANISH);

        let (calls, mut stub) = stub_with(|_| Ok(ENGLISH.into()));
        let prompt = Prompt::compose_mode("Write in English", "Write what I say in English.", &[]);
        let out = refine_with(&mut stub, &prompt, &on(), DANISH);
        assert_eq!(out.status, RefineStatus::Refined);
        assert_eq!(out.text, format!("{ENGLISH} "));
        assert_eq!(out.raw, DANISH);
        // No spoken instruction was involved: the mode asked for it.
        assert!(out.instruction.is_empty());
        let c = calls.lock().unwrap();
        assert!(c.last_system.contains("Write what I say in English."));
        assert_eq!(c.last_user, format!("<transcript>\n{}\n</transcript>", DANISH.trim()));
    }

    #[test]
    fn a_rewrite_still_needs_text_and_a_sane_length() {
        let prompt = Prompt::compose_mode("Formal", "Rewrite it in a formal tone.", &[]);
        let (_, mut stub) = stub_with(|_| Ok("  ".into()));
        let out = refine_with(&mut stub, &prompt, &on(), DANISH);
        assert_eq!(out.status, RefineStatus::Rejected);
        assert_eq!(out.text, DANISH);

        let (_, mut stub) = stub_with(|_| Ok("We regret to inform you of the delay. ".repeat(30)));
        let out = refine_with(&mut stub, &prompt, &on(), DANISH);
        assert_eq!(out.status, RefineStatus::Rejected);
        assert_eq!(out.text, DANISH);
    }

    #[test]
    fn a_rewrite_mode_sends_short_utterances_and_dictation_does_not() {
        let said = "tak for hjælpen ";
        let (calls, mut stub) = stub_with(|_| Ok("Thanks for the help.".into()));
        let prompt = Prompt::compose_mode("Write in English", "Write what I say in English.", &[]);
        let out = refine_with(&mut stub, &prompt, &on(), said);
        assert_eq!(out.status, RefineStatus::Refined);
        assert_eq!(out.text, "Thanks for the help. ");
        assert_eq!(calls.lock().unwrap().refined.len(), 1);

        // Nothing at all is still not sent.
        let out = refine_with(&mut stub, &prompt, &on(), "   ");
        assert_eq!(out.status, RefineStatus::SkippedShort);
        assert_eq!(calls.lock().unwrap().refined.len(), 1);

        let (calls, mut stub) = stub_with(|_| Ok("should not be used".into()));
        let out = refine_with(&mut stub, &Prompt::compose("", &[]), &on(), said);
        assert_eq!(out.status, RefineStatus::SkippedShort);
        assert!(calls.lock().unwrap().refined.is_empty());
    }

    #[test]
    fn a_spoken_instruction_is_still_declared_in_a_mode() {
        let said = "hej anna mødet er på torsdag nej vent fredag ";
        let (_, mut stub) =
            stub_with(|_| Ok("[instruction: nej vent]\nHi Anna, the meeting is on Friday.".into()));
        let prompt = Prompt::compose_mode("Write in English", "Write what I say in English.", &[]);
        let out = refine_with(&mut stub, &prompt, &on(), said);
        assert_eq!(out.status, RefineStatus::Refined);
        assert_eq!(out.instruction, "nej vent");
        assert_eq!(out.text, "Hi Anna, the meeting is on Friday. ");
    }

    #[test]
    fn with_refine_off_no_mode_reaches_a_backend() {
        let calls = Arc::new(Mutex::new(Calls::default()));
        let made = Arc::new(Mutex::new(0usize));
        let (c, m) = (calls.clone(), made.clone());
        // Refine off, a custom mode selected and an app default that
        // matches the app in front.
        let _scope = scope(RefineSettings::default(), move |_| {
            *m.lock().unwrap() += 1;
            Box::new(Stub::new(c.clone(), |_| Ok(ENGLISH.into())))
        });
        let modes = modes_with("write-in-english");
        let modes = crate::modes::set_app_default(&modes, "com.example.mail", "Mail", "formal").unwrap();
        crate::settings::set_modes_settings_cache(modes);
        test_support::set_frontmost_app(Some(("com.example.mail", "Mail")));

        warm_for_recording();
        assert!(!will_call_backend(DANISH));
        let out = run_for_dictation(DANISH);
        assert_eq!(out.status, RefineStatus::Disabled);
        assert_eq!(out.text, DANISH);
        assert_eq!(*made.lock().unwrap(), 0, "no backend may be built when off");
        let c = calls.lock().unwrap();
        assert_eq!((c.warmed, c.refined.len()), (0, 0));
    }

    #[test]
    fn the_pipeline_runs_the_selected_mode_warm_and_cold() {
        let calls = Arc::new(Mutex::new(Calls::default()));
        let built: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(Vec::new()));
        let (c, b) = (calls.clone(), built.clone());
        let _scope = scope(on(), move |settings| {
            b.lock()
                .unwrap()
                .push((settings.active_model().to_string(), settings.active_effort().to_string()));
            Box::new(Stub::new(c.clone(), |_| Ok(ENGLISH.into())))
        });
        crate::settings::set_modes_settings_cache(modes_with("write-in-english"));

        // Warmed at record start.
        warm_for_recording();
        assert!(will_call_backend("tak "), "a rewrite mode sends short text too");
        let warm = run_for_dictation(DANISH);
        // No warm backend: resolved when the transcript is ready.
        let cold = run_for_dictation(DANISH);

        for out in [&warm, &cold] {
            assert_eq!(out.status, RefineStatus::Refined);
            assert_eq!(out.text, format!("{ENGLISH} "));
            assert_eq!(out.mode.id, "write-in-english");
            assert_eq!(out.mode.name, "Write in English");
            assert_eq!(out.mode.reason, "selected");
            assert_eq!(out.mode.app_name, "");
            // The mode's own model and effort were used and reported.
            assert_eq!((out.model.as_str(), out.effort.as_str()), ("opus", "high"));
        }
        assert_eq!(
            *built.lock().unwrap(),
            vec![("opus".to_string(), "high".to_string()); 2],
            "both backends were built with the mode's model and effort"
        );
        let c = calls.lock().unwrap();
        assert_eq!(c.warmed, 1);
        assert!(c.last_system.contains("Write what I say in English."));
        assert!(c.last_system.contains("writing mode called \"Write in English\""));
    }

    #[test]
    fn the_app_default_wins_over_the_selected_mode_and_is_recorded() {
        let calls = Arc::new(Mutex::new(Calls::default()));
        let c = calls.clone();
        let _scope = scope(on(), move |_| Box::new(Stub::new(c.clone(), |_| Ok(ENGLISH.into()))));
        let modes = modes_with("formal");
        let modes =
            crate::modes::set_app_default(&modes, "com.example.mail", "Mail", "write-in-english").unwrap();
        crate::settings::set_modes_settings_cache(modes);

        // Dictating into the app with a default, warm and cold.
        test_support::set_frontmost_app(Some(("com.example.mail", "Mail")));
        warm_for_recording();
        let warm = run_for_dictation(DANISH);
        let cold = run_for_dictation(DANISH);
        for out in [&warm, &cold] {
            assert_eq!(out.status, RefineStatus::Refined);
            assert_eq!(out.mode.name, "Write in English");
            assert_eq!(out.mode.reason, "app-default");
            assert_eq!(out.mode.app_name, "Mail");
        }

        // Another app: the selected mode.
        test_support::set_frontmost_app(Some(("com.example.notes", "Notes")));
        warm_for_recording();
        let out = run_for_dictation(DANISH);
        assert_eq!((out.mode.name.as_str(), out.mode.reason.as_str()), ("Formal", "selected"));
        assert!(calls.lock().unwrap().last_system.contains("formal, polite tone"));

        // The app is not known (or is OpenWhisper itself): selected.
        test_support::set_frontmost_app(None);
        let out = run_for_dictation(DANISH);
        assert_eq!((out.mode.name.as_str(), out.mode.reason.as_str()), ("Formal", "selected"));
    }

    #[test]
    fn refine_text_in_mode_is_what_the_cli_runs() {
        let calls = Arc::new(Mutex::new(Calls::default()));
        let c = calls.clone();
        let _scope = scope(RefineSettings::default(), move |_| {
            Box::new(Stub::new(c.clone(), |_| Ok(ENGLISH.into())))
        });
        // On demand: works with Refine off, like `refine_text`.
        let settings = RefineSettings::default();
        let modes = modes_with("dictation");
        let pick = crate::modes::resolve(
            &crate::modes::set_app_default(&modes, "mail.exe", "", "write-in-english").unwrap(),
            &settings,
            Some(("mail.exe", "")),
        );
        let out = refine_text_in_mode(&settings, &pick, DANISH);
        assert_eq!(out.status, RefineStatus::Refined);
        assert_eq!(out.mode.reason, "app-default");
        assert_eq!(out.mode.app_name, "mail.exe");
        assert!(calls.lock().unwrap().last_system.contains("Write what I say in English."));

        // The built-in mode through the same door is plain Refine.
        let out = refine_text_in_mode(&settings, &choice(&modes, &settings), DANISH);
        assert_eq!(out.status, RefineStatus::Rejected, "a translation is drift in Dictation");
        assert_eq!(out.mode, ModeUse::default());
    }

    #[test]
    fn connection_test_reports_success_and_failure() {
        {
            let _scope = scope(RefineSettings::default(), |_| {
                Box::new(Stub::new(Arc::new(Mutex::new(Calls::default())), |_| {
                    Ok("This is a test of the Refine step. It should come back with punctuation."
                        .into())
                }))
            });
            // Works with Refine switched off: the test is on demand.
            let t = test_connection(&RefineSettings::default());
            assert!(t.ok, "{t:?}");
            assert!(t.output.starts_with("This is a test"));
            assert!(t.message.is_empty());
        }
        {
            let _scope = scope(RefineSettings::default(), |_| {
                Box::new(Stub::new(Arc::new(Mutex::new(Calls::default())), |_| {
                    Err(RefineError::Unavailable(
                        "Claude Code is not signed in.".into(),
                    ))
                }))
            });
            let t = test_connection(&RefineSettings::default());
            assert!(!t.ok);
            assert!(t.output.is_empty());
            assert_eq!(t.message, "Claude Code is not signed in.");
        }
    }

    #[test]
    fn status_ids_match_serialized_form() {
        for s in [
            RefineStatus::Disabled,
            RefineStatus::Refined,
            RefineStatus::SkippedShort,
            RefineStatus::Timeout,
            RefineStatus::Unavailable,
            RefineStatus::Failed,
            RefineStatus::Rejected,
        ] {
            assert_eq!(
                serde_json::to_string(&s).unwrap(),
                format!("\"{}\"", s.as_str())
            );
        }
    }
}
