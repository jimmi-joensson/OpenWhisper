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
//!   [`backend_status`] and [`test_connection`] with explicit
//!   settings.

use std::fmt;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::settings::{RefineBackendKind, RefineSettings};

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
    if words < settings.min_words as usize || input.is_empty() {
        return RefineOutcome::passthrough(
            text,
            RefineStatus::SkippedShort,
            format!(
                "{words} word{}, fewer than {}: not sent",
                if words == 1 { "" } else { "s" },
                settings.min_words
            ),
        );
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
    };

    let outcome = match result {
        Ok(answer) => {
            // A rewrite is only accepted when the model names the
            // spoken instruction that asked for it. Without one, the
            // text must still be what was said (strict guard).
            let (instruction, body) = prompt::split_instruction(&answer);
            let cleaned = prompt::clean_model_output(body, input);
            let verdict = match &instruction {
                Some(_) => guard::check_instructed(input, &cleaned),
                None => guard::check(input, &cleaned, settings.drift_guard),
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

/// Refine `text` with a fresh backend built from `settings`. For the
/// CLI and one-off calls; the dictation pipeline uses
/// [`run_for_dictation`], which can reuse a warmed backend.
pub fn refine_text(settings: &RefineSettings, text: &str) -> RefineOutcome {
    let prompt = compose_prompt(settings);
    let mut backend = backend_for(settings);
    refine_with(backend.as_mut(), &prompt, settings, text)
}

// --- dictation pipeline hooks -------------------------------------

struct WarmSlot {
    backend: Box<dyn Refiner>,
    prompt: Prompt,
    settings: RefineSettings,
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

/// Called when capture starts. With Refine on, builds the backend
/// and lets it start up while the user talks (the Claude CLI takes
/// over a second to boot; warmed, the answer comes back in well under
/// one). With Refine off this does nothing at all.
///
/// Reads the cached settings ([`crate::settings::current_refine_settings`]).
/// May block briefly (dictionary read, keychain read, process spawn),
/// so call it off the hotkey thread.
pub fn warm_for_recording() {
    let settings = crate::settings::current_refine_settings();
    // Whatever was left from an earlier recording goes first.
    drop(take_warm());
    if !settings.enabled {
        return;
    }
    let epoch = EPOCH.load(Ordering::SeqCst);
    let started = Instant::now();
    let prompt = compose_prompt(&settings);
    let mut backend = backend_for(&settings);
    backend.warm(&prompt);
    crate::verbose_log!(
        "[ow.refine] warm backend={} setup_ms={} (own thread, recording already running)",
        backend.id(),
        started.elapsed().as_millis()
    );
    if let Ok(mut slot) = WARM.lock() {
        if EPOCH.load(Ordering::SeqCst) == epoch {
            *slot = Some(WarmSlot {
                backend,
                prompt,
                settings,
            });
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
/// with the settings that were current at that moment; otherwise
/// builds a cold backend from the current settings.
///
/// With Refine off, returns [`RefineOutcome::disabled`] and starts
/// nothing.
pub fn run_for_dictation(text: &str) -> RefineOutcome {
    let outcome = match take_warm() {
        Some(mut slot) => refine_with(slot.backend.as_mut(), &slot.prompt, &slot.settings, text),
        None => {
            let settings = crate::settings::current_refine_settings();
            if !settings.enabled {
                return RefineOutcome::disabled(text);
            }
            refine_text(&settings, text)
        }
    };
    history::record_outcome(history::SOURCE_DICTATION, &outcome);
    outcome
}

/// True when the pipeline should show a "refining" status for this
/// transcript: Refine is on and the text is long enough to be sent.
pub fn will_call_backend(text: &str) -> bool {
    let settings = match WARM.lock() {
        Ok(g) => g.as_ref().map(|s| s.settings.clone()),
        Err(_) => None,
    }
    .unwrap_or_else(crate::settings::current_refine_settings);
    settings.enabled && guard::word_count(text) >= settings.min_words as usize
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
    /// Serializes tests that touch the process-wide warm slot, the
    /// settings cache or the factory override.
    static LOCK: Mutex<()> = Mutex::new(());

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
            crate::settings::set_refine_settings_cache(RefineSettings::default());
        }
    }

    /// Install `make` as the backend factory and `settings` as the
    /// current settings for the lifetime of the returned scope.
    pub(crate) fn scope(
        settings: RefineSettings,
        make: impl Fn(&RefineSettings) -> Box<dyn Refiner> + Send + Sync + 'static,
    ) -> Scope {
        let guard = LOCK.lock().unwrap_or_else(|p| p.into_inner());
        super::discard_warm();
        if let Ok(mut g) = FACTORY.lock() {
            *g = Some(Arc::new(make));
        }
        crate::settings::set_refine_settings_cache(settings);
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
