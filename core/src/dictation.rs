//! Dictation state machine, platform-agnostic.
//!
//! Owns phase transitions and the status strings shown in the UI. Shells
//! (Swift on macOS, C# on Windows) drive this by:
//!   1. Calling [`dictation_request_toggle`] / [`dictation_request_cancel`]
//!      on user intent. Core decides whether the action is valid and what
//!      the shell should do next (load/capture/transcribe).
//!   2. Reporting progress with `mark_*` / `deliver_*` entry points as the
//!      shell actually does the work.
//!   3. Polling [`dictation_snapshot`] to drive the view layer.
//!
//! Sample buffers deliberately stay out of core state in host-driven STT
//! mode (macOS today, Windows if we ever run ORT outside the core): the
//! shell already holds the samples and passes them straight to its engine.
//! When a fully native-Rust STT path lands (task 3 — sherpa-onnx on
//! Windows), it'll live behind a trait that drains audio internally and
//! calls `deliver_transcript` without round-tripping through the shell.
//!
//! ## Two kinds of recording
//!
//! The same recorder serves two purposes ([`SessionKind`]). A
//! *dictation* is transcribed, cleaned, refined and typed where the
//! cursor is: everything described above. An *ask* (TASK-97) is a
//! question for the model: it is transcribed and cleaned the same
//! way, and then handed to [`crate::ask`] instead. It is never
//! refined, never given to the injector, never counted in the stats
//! and never written to the dictation history, and it leaves the
//! transcript of the last dictation on the snapshot as it was.
//!
//! A recording keeps the kind it was started with, whichever key
//! stops it. Starting one is [`run_toggle_kind`]; [`run_toggle`] is
//! the dictation it always was. Without the `ask` feature compiled in
//! there are no asks: an ask toggle starts nothing.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

// Phase values exposed across FFI as u32. Keep in sync with the Swift/C#
// mirror enums. Only unit variants — no associated data — keeps swift-bridge
// happy and lets the error message travel as a separate String field.
pub const PHASE_IDLE: u32 = 0;
pub const PHASE_LOADING_MODEL: u32 = 1;
pub const PHASE_RECORDING: u32 = 2;
pub const PHASE_TRANSCRIBING: u32 = 3;
pub const PHASE_DONE: u32 = 4;
pub const PHASE_ERROR: u32 = 5;

// ToggleAction values — what the shell should do after a toggle request.
pub const TOGGLE_IGNORE: u32 = 0;
pub const TOGGLE_BEGIN_RECORDING: u32 = 1;
pub const TOGGLE_STOP_RECORDING: u32 = 2;

/// What a recording is for. See the module docs.
#[non_exhaustive]
#[derive(serde::Serialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SessionKind {
    /// The words are typed where the cursor is.
    #[default]
    Dictation,
    /// The words are a question for the model; the answer is shown in
    /// a panel.
    Ask,
}

impl SessionKind {
    /// Stable id, same as the serialized form.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dictation => "dictation",
            Self::Ask => "ask",
        }
    }
}

struct State {
    phase: u32,
    /// What the recording in flight (or the last one) is for.
    kind: SessionKind,
    /// The ask the recording in flight belongs to
    /// ([`crate::ask::mark_listening`]); 0 for none.
    ask_id: u64,
    status_message: String,
    transcript: String,
    /// Dictionary fixes made in `transcript` (see
    /// [`crate::transcript::process_tracked`]). Cleared with it.
    replacements: Vec<crate::dictionary::Replacement>,
    /// What the Refine step did to `transcript` (see
    /// [`dictation_set_refine_result`]). Cleared with it.
    refine: RefineInfo,
    confidence: f32,
    sample_count: u64,
    record_start: Option<Instant>,
    /// Wall-clock unix-epoch milliseconds at the moment recording began.
    /// Captured alongside `record_start` so the stats writer has an
    /// absolute timestamp to put into `dictations.started_at`. `Instant`
    /// can't be converted to wall time directly, hence the parallel
    /// field. Reset on every transition that clears `record_start`.
    record_start_epoch_ms: Option<i64>,
    /// Active-speech milliseconds reported by the shell after audio
    /// drain (see [`dictation_set_voiced_ms`]). When `Some`, the stats
    /// writer uses it instead of wall-clock duration so silence at
    /// the tail of a recording doesn't count against Time Saved.
    /// Reset on every transition that clears `record_start`.
    voiced_ms_at_drain: Option<i64>,
    error_message: String,
    /// Bytes downloaded so far for the current model fetch. 0 when no
    /// download is in progress. Reset to 0 on `mark_loaded` / error so
    /// the UI doesn't keep a stale 100 % bar around after success.
    download_bytes_done: u64,
    /// Total bytes expected for the current model fetch (Content-Length).
    /// 0 when unknown or no download in progress.
    download_bytes_total: u64,
}

/// Result of the Refine step for the current transcript, as the UI
/// needs it. Plain strings so this module compiles (and the snapshot
/// keeps its shape) without the `refine` feature.
#[derive(serde::Serialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct RefineInfo {
    /// The transcript before Refine (after rules and dictionary).
    /// Empty when Refine did not run.
    pub raw_transcript: String,
    /// `""` when Refine did not run, otherwise the status id:
    /// `refined`, `skipped-short`, `timeout`, `unavailable`,
    /// `failed`, `rejected`.
    pub status: String,
    /// Why the transcript was not refined; empty otherwise.
    pub detail: String,
    /// The spoken instruction Refine carried out ("make that more
    /// formal"), in the speaker's words; empty for plain cleanup.
    pub instruction: String,
    /// Backend id, e.g. `claude-cli`.
    pub backend: String,
    /// Time spent waiting for the backend.
    pub elapsed_ms: u64,
    /// Name of the mode the dictation ran in (TASK-96), when there is
    /// something to say about it: a custom mode, or any mode,
    /// Dictation included, that ran because the app has it as its
    /// default (so "which mode, and why" is never blank when an app
    /// default overrode the selected mode). Empty for Dictation as
    /// the selected mode and when Refine did not run.
    pub mode: String,
    /// `mode` names the built-in Dictation mode. The history file
    /// only records a custom mode, so it reads this.
    pub mode_builtin: bool,
    /// Why that mode ran: `selected` or `app-default`. Empty when
    /// Refine did not run.
    pub mode_reason: String,
    /// The app whose default mode it was; empty unless `mode_reason`
    /// is `app-default`.
    pub mode_app: String,
}

#[cfg(feature = "refine")]
impl RefineInfo {
    /// What the UI is told about a Refine run. `None` when Refine is
    /// off: then nothing ran and there is nothing to show.
    pub fn from_outcome(outcome: &crate::refine::RefineOutcome) -> Option<Self> {
        if outcome.status == crate::refine::RefineStatus::Disabled {
            return None;
        }
        let mode = &outcome.mode;
        let by_app = mode.reason == crate::modes::ModeReason::AppDefault.as_str();
        Some(Self {
            raw_transcript: outcome.raw.clone(),
            status: outcome.status.as_str().to_string(),
            detail: outcome.detail.clone(),
            instruction: outcome.instruction.clone(),
            backend: outcome.backend.clone(),
            elapsed_ms: outcome.elapsed_ms,
            // Dictation as the selected mode is the plain case and
            // gets no label. Dictation because the app says so does:
            // it may have overridden a custom mode the user selected.
            mode: if mode.is_builtin() && !by_app { String::new() } else { mode.name.clone() },
            mode_builtin: mode.is_builtin(),
            mode_reason: mode.reason.clone(),
            mode_app: mode.app_name.clone(),
        })
    }
}

impl State {
    fn new() -> Self {
        Self {
            phase: PHASE_IDLE,
            kind: SessionKind::Dictation,
            ask_id: 0,
            status_message: "idle — tap Record, speak, tap again".to_string(),
            transcript: String::new(),
            replacements: Vec::new(),
            refine: RefineInfo::default(),
            confidence: 0.0,
            sample_count: 0,
            record_start: None,
            record_start_epoch_ms: None,
            voiced_ms_at_drain: None,
            error_message: String::new(),
            download_bytes_done: 0,
            download_bytes_total: 0,
        }
    }

    fn can_toggle(&self) -> bool {
        matches!(
            self.phase,
            PHASE_IDLE | PHASE_DONE | PHASE_ERROR | PHASE_RECORDING
        )
    }
}

/// Snapshot object returned across FFI. Owned by Rust, accessed via
/// swift-bridge opaque type methods from Swift — avoids field tearing
/// because the whole struct is captured under one mutex lock.
#[non_exhaustive]
pub struct DictationSnapshot {
    phase: u32,
    session_kind: SessionKind,
    status_message: String,
    transcript: String,
    replacements: Vec<crate::dictionary::Replacement>,
    refine: RefineInfo,
    confidence: f32,
    sample_count: u64,
    elapsed_ms: u64,
    error_message: String,
    can_toggle: bool,
    is_recording: bool,
    download_bytes_done: u64,
    download_bytes_total: u64,
}

impl DictationSnapshot {
    pub fn phase(&self) -> u32 {
        self.phase
    }
    /// What the recording in flight is for, or what the last one was
    /// for. The pill shows an ask recording differently, and the
    /// transcript on this snapshot is never that of an ask. Rust-only
    /// (not part of the swift-bridge surface).
    pub fn session_kind(&self) -> SessionKind {
        self.session_kind
    }
    pub fn status_message(&self) -> String {
        self.status_message.clone()
    }
    pub fn transcript(&self) -> String {
        self.transcript.clone()
    }
    /// Dictionary fixes in [`DictationSnapshot::transcript`]. Rust-only
    /// (not part of the swift-bridge surface).
    pub fn replacements(&self) -> Vec<crate::dictionary::Replacement> {
        self.replacements.clone()
    }
    /// What the Refine step did to [`DictationSnapshot::transcript`],
    /// including the unrefined text. Rust-only (not part of the
    /// swift-bridge surface).
    pub fn refine(&self) -> RefineInfo {
        self.refine.clone()
    }
    pub fn confidence(&self) -> f32 {
        self.confidence
    }
    pub fn sample_count(&self) -> u64 {
        self.sample_count
    }
    pub fn elapsed_ms(&self) -> u64 {
        self.elapsed_ms
    }
    pub fn error_message(&self) -> String {
        self.error_message.clone()
    }
    pub fn can_toggle(&self) -> bool {
        self.can_toggle
    }
    pub fn is_recording(&self) -> bool {
        self.is_recording
    }
    pub fn download_bytes_done(&self) -> u64 {
        self.download_bytes_done
    }
    pub fn download_bytes_total(&self) -> u64 {
        self.download_bytes_total
    }

    /// Sample count to surface in the dictation tick payload. While the
    /// phase is RECORDING, `State::sample_count` is 0 until the capture
    /// drain happens on stop — so the UI counter would sit at 0 the whole
    /// recording. Derive a running count from `elapsed_ms` and the
    /// caller's known capture rate instead. After stop, fall through to
    /// the real sample count.
    pub fn live_samples(&self, sample_rate_hz: u64) -> u64 {
        if self.phase == PHASE_RECORDING {
            self.elapsed_ms * sample_rate_hz / 1000
        } else {
            self.sample_count
        }
    }
}

/// Stable UI status string for a phase value. Three buckets: `"recording"`,
/// `"transcribing"`, and `"idle"` for everything else (idle / loading /
/// done / error). The dictation tick payload includes this so the UI
/// doesn't need to mirror the phase-to-label mapping.
pub fn phase_status_label(phase: u32) -> &'static str {
    match phase {
        PHASE_RECORDING => "recording",
        PHASE_TRANSCRIBING => "transcribing",
        _ => "idle",
    }
}

/// Decision returned by [`fullscreen_action`] — what the shell should do
/// when a fullscreen-state transition is detected. Pure data; no Tauri /
/// AppKit / Win32 types so the same return value is consumed by the
/// SwiftUI shell, the Tauri shell, and unit tests.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FullscreenAction {
    /// Pill HUD should hide while fullscreen is active.
    pub hide_pill: bool,
    /// Global hotkey should be detached while fullscreen is active.
    pub detach_hotkey: bool,
    /// A recording was in flight at the moment fullscreen kicked in; the
    /// shell should call its cancel path so the transcript doesn't surprise-
    /// paste into the fullscreen app. Always implies `hide_pill`.
    pub cancel_recording: bool,
}

impl FullscreenAction {
    pub fn new(hide_pill: bool, detach_hotkey: bool, cancel_recording: bool) -> Self {
        Self {
            hide_pill,
            detach_hotkey,
            cancel_recording,
        }
    }
}

/// Compute the fullscreen-state action without touching any platform API.
///
/// Inputs: whether the foreground app is fullscreen now, whether the user
/// has opted into showing the pill in fullscreen (Settings → Behavior →
/// "Show pill in fullscreen apps"), and whether the dictation state
/// machine currently has an active recording.
///
/// The decision is the conjunction of "fullscreen detected" and "user has
/// not opted out". When suppressed, the pill hides, the global hotkey
/// detaches, and an in-flight recording is silently aborted.
pub fn fullscreen_action(
    is_fullscreen: bool,
    show_in_fullscreen: bool,
    is_recording: bool,
) -> FullscreenAction {
    let suppress = is_fullscreen && !show_in_fullscreen;
    FullscreenAction {
        hide_pill: suppress,
        detach_hotkey: suppress,
        cancel_recording: suppress && is_recording,
    }
}

static STATE: OnceLock<Mutex<State>> = OnceLock::new();

/// Lock-free mirror of `phase == PHASE_RECORDING`. OS-level hot paths
/// (Windows `WH_KEYBOARD_LL` callback with a 300 ms LowLevelHooksTimeout
/// budget; macOS `CGEventTap` callback with a ~1 s budget) read this on
/// every key event to decide whether to swallow Escape — taking the
/// dictation Mutex there is allowed but unnecessary, and a stuck lock
/// would silently unload the Windows hook. Updated by the same code paths
/// that flip `State::phase`.
static IS_RECORDING: AtomicBool = AtomicBool::new(false);

/// Read-only accessor for the recording mirror. Safe to call from any
/// thread, including OS-level keyboard hooks.
pub fn is_recording() -> bool {
    IS_RECORDING.load(Ordering::Relaxed)
}

fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mutex = STATE.get_or_init(|| Mutex::new(State::new()));
    let mut guard = mutex.lock().expect("dictation state poisoned");
    f(&mut guard)
}

pub fn dictation_snapshot() -> DictationSnapshot {
    with_state(|s| DictationSnapshot {
        phase: s.phase,
        session_kind: s.kind,
        status_message: s.status_message.clone(),
        transcript: s.transcript.clone(),
        replacements: s.replacements.clone(),
        refine: s.refine.clone(),
        confidence: s.confidence,
        sample_count: s.sample_count,
        elapsed_ms: s
            .record_start
            .map(|t| t.elapsed().as_millis() as u64)
            .unwrap_or(0),
        error_message: s.error_message.clone(),
        can_toggle: s.can_toggle(),
        is_recording: s.phase == PHASE_RECORDING,
        download_bytes_done: s.download_bytes_done,
        download_bytes_total: s.download_bytes_total,
    })
}

/// Snapshot for the crash inspector. Reads the dictation state via
/// `try_lock` so a panic inside the holder of the state lock does not
/// deadlock the panic hook.
///
/// Returns `None` when state is uninitialized, the lock is held by the
/// panicker, OR the phase is `IDLE` (crash was outside a dictation —
/// per spec, `recording_state` should be `null` in that case).
pub fn try_snapshot_for_crash() -> Option<crate::crashes::RecordingStateSnapshot> {
    let mutex = STATE.get()?;
    let guard = mutex.try_lock().ok()?;
    if guard.phase == PHASE_IDLE {
        return None;
    }
    let duration_ms = guard
        .record_start
        .map(|t| t.elapsed().as_millis() as u64)
        .unwrap_or(0);
    Some(crate::crashes::RecordingStateSnapshot {
        status_message_at_crash: guard.status_message.clone(),
        duration_ms,
        samples_captured: guard.sample_count,
        // Plumbing the active engine name + hashed device id is follow-up
        // scope; v1 schema permits these to be `null` and the inspector
        // handles missing values.
        model_kind: None,
        device_id_hash: None,
    })
}

pub fn dictation_request_toggle() -> u32 {
    dictation_request_toggle_kind(SessionKind::Dictation)
}

/// [`dictation_request_toggle`] for a recording of the given kind.
/// `kind` only matters when the toggle starts a recording: one that
/// is running is stopped whichever kind asks, and keeps its own.
///
/// Starting an ask leaves the transcript, the dictionary fixes and
/// the Refine result of the last dictation where they are: they
/// belong to that dictation, and an ask puts nothing in their place.
pub fn dictation_request_toggle_kind(kind: SessionKind) -> u32 {
    with_state(|s| {
        if !s.can_toggle() {
            return TOGGLE_IGNORE;
        }
        if s.phase == PHASE_RECORDING {
            TOGGLE_STOP_RECORDING
        } else {
            s.kind = kind;
            s.ask_id = 0;
            if kind == SessionKind::Dictation {
                // Fresh session: clear previous transcript/error so the UI
                // doesn't briefly show stale content while the shell spins up.
                s.transcript.clear();
                s.replacements.clear();
                s.refine = RefineInfo::default();
                s.confidence = 0.0;
            }
            s.sample_count = 0;
            s.error_message.clear();
            s.status_message = "preparing…".to_string();
            TOGGLE_BEGIN_RECORDING
        }
    })
}

pub fn dictation_request_cancel() -> bool {
    let cancelled = with_state(|s| {
        if s.phase != PHASE_RECORDING {
            return false;
        }
        s.record_start = None;
        s.record_start_epoch_ms = None;
        s.voiced_ms_at_drain = None;
        // A cancelled ask recording had put nothing there to clear:
        // what is on the snapshot is still the last dictation.
        if s.kind == SessionKind::Dictation {
            s.transcript.clear();
            s.replacements.clear();
            s.refine = RefineInfo::default();
            s.confidence = 0.0;
        }
        s.sample_count = 0;
        s.status_message = "cancelled".to_string();
        s.phase = PHASE_IDLE;
        true
    });
    if cancelled {
        IS_RECORDING.store(false, Ordering::Relaxed);
    }
    cancelled
}

pub fn dictation_mark_loading_model() {
    IS_RECORDING.store(false, Ordering::Relaxed);
    with_state(|s| {
        s.phase = PHASE_LOADING_MODEL;
        // Neutral default — recognizer will overwrite to "downloading…"
        // once it sees a missing cache (and the % updates flow in via
        // `dictation_set_download_progress`), or to "loading model into
        // memory…" once it reaches session build. Cached-model boots stay
        // on this string for the brief window before session build kicks in.
        s.status_message = "loading model…".to_string();
    });
    crate::crashes::event_buffer::push_event(
        "PhaseChange",
        serde_json::json!({ "to": "LoadingModel" }),
    );
}

/// Bridge for the recognizer's download path. Callers may invoke this many
/// times per second (one per chunk write), so it must stay cheap. `total = 0`
/// means Content-Length wasn't reported — UI shows an indeterminate state.
pub fn dictation_set_download_progress(done: u64, total: u64) {
    set_progress_internal("downloading model", done, total);
}

/// Bridge for the recognizer's archive-extract path. Same shape as
/// download progress (drives the same bar), different verb. `done` is
/// bytes consumed from the compressed archive — gives a roughly linear
/// fill since bzip2 decompression is CPU-bound and reads the input
/// sequentially.
pub fn dictation_set_extract_progress(done: u64, total: u64) {
    set_progress_internal("extracting model", done, total);
}

fn set_progress_internal(verb: &str, done: u64, total: u64) {
    with_state(|s| {
        s.download_bytes_done = done;
        s.download_bytes_total = total;
        if total > 0 {
            let pct = ((done as f64 / total as f64) * 100.0).clamp(0.0, 100.0);
            let done_mb = done / 1_048_576;
            let total_mb = total / 1_048_576;
            s.status_message = format!("{verb}… {done_mb}/{total_mb} MB ({pct:.0}%)");
        } else {
            s.status_message = format!("{verb}…");
        }
    })
}

/// Recognizer is fully loaded (sessions built, ready to transcribe). Clears
/// the download progress and returns the phase to IDLE — but only if it's
/// still LOADING_MODEL, so a user-initiated transition (started recording
/// while warmup was in flight) isn't clobbered.
pub fn dictation_mark_loaded() {
    with_state(|s| {
        s.download_bytes_done = 0;
        s.download_bytes_total = 0;
        if s.phase == PHASE_LOADING_MODEL {
            s.phase = PHASE_IDLE;
            s.status_message = "idle — tap Record, speak, tap again".to_string();
        }
    });
    crate::crashes::event_buffer::push_event("ModelLoaded", serde_json::json!({}));
}

/// Recognizer has finished downloading the archive and is now extracting /
/// loading sessions. Status string only — no phase transition (still LOADING).
pub fn dictation_mark_loading_session() {
    with_state(|s| {
        s.download_bytes_done = 0;
        s.download_bytes_total = 0;
        s.status_message = "loading model into memory…".to_string();
    })
}

pub fn dictation_mark_capture_started() {
    with_state(|s| {
        s.phase = PHASE_RECORDING;
        s.status_message = "recording — tap again to stop".to_string();
        s.record_start = Some(Instant::now());
        s.record_start_epoch_ms = Some(now_epoch_ms());
    });
    IS_RECORDING.store(true, Ordering::Relaxed);
    crate::crashes::event_buffer::push_event("DictationStart", serde_json::json!({}));
}

fn now_epoch_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Optimistic phase flip the shell calls the instant the stop hotkey
/// fires — *before* draining/resampling the audio buffer. Without this,
/// the heavy sinc resample (~1–2 s for a 30 s clip on Windows) runs on
/// the hotkey thread and the UI sits frozen on "recording" until it
/// finishes. Pairs with a follow-up [`dictation_mark_capture_stopped`]
/// call once the worker has the sample count, which corrects the empty
/// case (mic produced nothing → PHASE_DONE) without re-touching phase
/// in the populated case.
pub fn dictation_mark_transcribing_pending() {
    IS_RECORDING.store(false, Ordering::Relaxed);
    with_state(|s| {
        s.phase = PHASE_TRANSCRIBING;
        s.status_message = "transcribing…".to_string();
    });
    crate::crashes::event_buffer::push_event(
        "PhaseChange",
        serde_json::json!({ "to": "Transcribing" }),
    );
}

/// Push the active-speech ms estimate from the shell. Called once per
/// recording, after `audio_drain_samples` and before
/// `dictation_deliver_transcript`. Stored in dictation state so
/// `deliver_transcript` can hand it to the stats writer instead of
/// wall-clock duration. Calling more than once per session overwrites
/// the prior value (last call wins) — should not happen in practice
/// since the stop pipeline drains exactly once.
pub fn dictation_set_voiced_ms(ms: i64) {
    with_state(|s| {
        s.voiced_ms_at_drain = Some(ms);
    });
}

pub fn dictation_mark_capture_stopped(sample_count: u64) {
    IS_RECORDING.store(false, Ordering::Relaxed);
    with_state(|s| {
        s.sample_count = sample_count;
        if sample_count == 0 {
            s.status_message = "no audio captured".to_string();
            s.phase = PHASE_DONE;
            s.record_start = None;
            s.record_start_epoch_ms = None;
        } else {
            s.phase = PHASE_TRANSCRIBING;
            s.status_message = "transcribing on ANE…".to_string();
        }
    })
}

/// The transcript is with the Refine backend. Status string only: the
/// phase stays TRANSCRIBING, so the pill and the toggle gating do not
/// change. Called only when a model call is actually about to happen.
pub fn dictation_mark_refining() {
    with_state(|s| {
        if s.phase == PHASE_TRANSCRIBING {
            s.status_message = "refining…".to_string();
        }
    });
    crate::crashes::event_buffer::push_event(
        "PhaseChange",
        serde_json::json!({ "to": "Refining" }),
    );
}

/// Record what the Refine step did, before the transcript is
/// delivered, so the DONE tick already carries it. `info.status`
/// empty means Refine did not run (switched off).
pub fn dictation_set_refine_result(info: RefineInfo) {
    with_state(|s| s.refine = info);
}

/// The Refine stage of the stop pipeline: rule-cleaned text in, the
/// text to deliver out. With Refine off (the default) this returns
/// `cleaned` unchanged and starts no process or request. Any failure
/// inside Refine also returns `cleaned`; the reason lands on the
/// snapshot (see [`RefineInfo`]).
#[cfg(all(feature = "recognizer", feature = "refine"))]
fn refine_stage(cleaned: &str) -> String {
    // The mode is worked out once, when the transcript is ready (see
    // `run_for_dictation_announced`); the status goes up from there,
    // only when the text is really sent.
    let outcome = crate::refine::run_for_dictation_announced(cleaned, dictation_mark_refining);
    if let Some(info) = RefineInfo::from_outcome(&outcome) {
        dictation_set_refine_result(info);
    }
    outcome.text
}

#[cfg(all(feature = "recognizer", not(feature = "refine")))]
fn refine_stage(cleaned: &str) -> String {
    cleaned.to_string()
}

/// Let the Refine backend start up while the user talks. No-op with
/// Refine off or the feature not compiled in.
#[cfg(feature = "recognizer")]
fn refine_warm<E: DictationEnv>(env: &E) {
    #[cfg(feature = "refine")]
    env.spawn(
        "openwhisper-refine-warm",
        Box::new(crate::refine::warm_for_recording),
    );
    #[cfg(not(feature = "refine"))]
    let _ = env;
}

/// Drop whatever [`refine_warm`] started (cancelled or empty
/// recording, failed transcription).
#[cfg(feature = "recognizer")]
fn refine_discard() {
    #[cfg(feature = "refine")]
    crate::refine::discard_warm();
}

// --- the ask kind ---------------------------------------------------
//
// Everything below is a no-op without the `ask` feature, the way the
// Refine hooks above are without `refine`.

/// An ask can be started: Ask is compiled in and switched on.
#[cfg(feature = "recognizer")]
fn ask_enabled() -> bool {
    #[cfg(feature = "ask")]
    {
        crate::settings::current_ask_settings().enabled
    }
    #[cfg(not(feature = "ask"))]
    {
        false
    }
}

/// The ask recording started: the ask is `listening` from this moment
/// (a cheap flag flip, so the panel can show it at once), and its
/// backend starts up on a thread of its own while the person talks,
/// in place of the Refine warm-up a dictation gets.
#[cfg(feature = "recognizer")]
fn ask_begin<E: DictationEnv>(env: &E) {
    #[cfg(feature = "ask")]
    match crate::ask::mark_listening() {
        Ok(id) => {
            with_state(|s| s.ask_id = id);
            env.spawn(
                "openwhisper-ask-warm",
                Box::new(move || crate::ask::warm_listening(id)),
            );
        }
        // Switched off in the moment since the toggle checked. The
        // recording runs to its end and is then dropped: its
        // transcript is offered to an ask that does not exist.
        Err(e) => crate::verbose_log!("[ow.dictation] ask recording without an ask: {e}"),
    }
    #[cfg(not(feature = "ask"))]
    let _ = env;
}

/// The ask is off: its recording was empty, or the person closed it.
/// With an `id`, only when the current ask is still that one, so a
/// close that lands late leaves a new ask alone; without, whatever
/// ask is there.
#[cfg(feature = "recognizer")]
fn ask_dismiss(id: Option<u64>) {
    #[cfg(feature = "ask")]
    match id {
        Some(id) => {
            crate::ask::dismiss_if(id);
        }
        None => crate::ask::dismiss(),
    }
    #[cfg(not(feature = "ask"))]
    let _ = id;
}

/// Stop the ask, if it is listening, thinking or answering. With an
/// `id`, only when the current ask is still that one; without,
/// whatever ask is there (the cancel key). See [`run_cancel_ask`].
#[cfg(feature = "recognizer")]
fn ask_cancel(id: Option<u64>) -> bool {
    #[cfg(feature = "ask")]
    {
        match id {
            Some(id) => crate::ask::cancel_if(id),
            None => crate::ask::cancel(),
        }
    }
    #[cfg(not(feature = "ask"))]
    {
        let _ = id;
        false
    }
}

/// The recording that just failed was for an ask: that ask ends in
/// its error phase with the reason, so the panel shows it instead of
/// staying on "listening". Does nothing for a dictation, and nothing
/// when the ask is not listening any more.
fn ask_fail(kind: SessionKind, ask_id: u64, message: &str) {
    #[cfg(feature = "ask")]
    if kind == SessionKind::Ask && ask_id != 0 {
        crate::ask::fail_listening(ask_id, &format!("The question could not be transcribed ({message})."));
    }
    #[cfg(not(feature = "ask"))]
    let _ = (kind, ask_id, message);
}

/// The ask recording is transcribed, so the dictation machine is done
/// with it: DONE, without a transcript of its own. Returns the ask
/// the recording was for and its voiced length.
#[cfg(feature = "recognizer")]
fn dictation_mark_ask_handed_over() -> (u64, i64) {
    IS_RECORDING.store(false, Ordering::Relaxed);
    with_state(|s| {
        let wall_clock_ms = s.record_start.map(|t| t.elapsed().as_millis() as i64).unwrap_or(0);
        let spoken_ms = s.voiced_ms_at_drain.take().unwrap_or(wall_clock_ms);
        s.status_message = "question sent".to_string();
        s.phase = PHASE_DONE;
        s.record_start = None;
        s.record_start_epoch_ms = None;
        (s.ask_id, spoken_ms)
    })
}

/// The ask branch of the stop pipeline: the transcript of an ask
/// recording goes through the rule pass and the dictionary like any
/// other, and then to [`crate::ask::submit_listening`] on this
/// thread, which returns when the answer is complete.
///
/// What it does not do is the point: no Refine, no injector, no stats
/// row, no dictation history entry, and nothing of the last dictation
/// on the snapshot (transcript, dictionary fixes, Refine result) is
/// touched. The ask keeps its own history and diagnostics.
#[cfg(feature = "recognizer")]
fn ask_stage(raw: &str) {
    // The fixes are not kept: they would sit next to a transcript
    // that is not this recording's.
    let (question, _fixes) = crate::transcript::process_tracked(raw);
    let (ask_id, spoken_ms) = dictation_mark_ask_handed_over();
    #[cfg(feature = "ask")]
    if let Err(e) = crate::ask::submit_listening(ask_id, &question, spoken_ms) {
        // Cancelled while it was being transcribed, or Ask was
        // switched off meanwhile. Nothing was sent.
        crate::verbose_log!("[ow.dictation] ask not sent: {e}");
    }
    #[cfg(not(feature = "ask"))]
    let _ = (question, ask_id, spoken_ms);
}

/// True when the cancel key has something to cancel: a recording, or
/// an ask that is being transcribed or answered. Lock-free, like
/// [`is_recording`], so the keyboard hooks can gate the cancel
/// binding on it and leave Escape to the focused app otherwise.
pub fn cancel_would_act() -> bool {
    #[cfg(feature = "ask")]
    {
        is_recording() || crate::ask::is_busy()
    }
    #[cfg(not(feature = "ask"))]
    {
        is_recording()
    }
}

/// Cancel an ask, whatever it is doing: what the cancel button of
/// the answer panel calls. An ask that is being recorded ends with
/// its recording ([`run_cancel`]); one that is being transcribed or
/// answered is stopped where it is (the model request is killed and
/// the ask shows as cancelled). A finished answer is left alone, and
/// so is a dictation that happens to be recording.
///
/// `id` is the ask the caller means, the one its panel shows. When
/// the current ask is another one by now, nothing happens. `None`
/// means whatever ask is there. Returns whether there was something
/// to stop.
#[cfg(feature = "recognizer")]
pub fn run_cancel_ask<E: DictationEnv>(env: &E, id: Option<u64>) -> bool {
    if recording_an_ask(id) {
        return run_cancel(env);
    }
    ask_cancel(id)
}

/// Close an ask: what closing the answer panel calls. Like
/// [`run_cancel_ask`], and then the ask is gone from the snapshot
/// altogether, finished answer included. `id` as there: a close that
/// was meant for an earlier ask does not touch the one that started
/// since.
#[cfg(feature = "recognizer")]
pub fn run_dismiss_ask<E: DictationEnv>(env: &E, id: Option<u64>) {
    if recording_an_ask(id) {
        let _ = run_cancel(env);
    }
    ask_dismiss(id);
}

/// The recording in flight is an ask: the ask `id`, when one is
/// given.
#[cfg(feature = "recognizer")]
fn recording_an_ask(id: Option<u64>) -> bool {
    is_recording() && with_state(|s| s.kind == SessionKind::Ask && id.is_none_or(|id| id == s.ask_id))
}

pub fn dictation_deliver_transcript(text: &str, confidence: f32) {
    IS_RECORDING.store(false, Ordering::Relaxed);
    let (started_at_ms, duration_ms, wall_clock_ms) = with_state(|s| {
        let started_at_ms = s.record_start_epoch_ms.unwrap_or(0);
        let wall_clock_ms = s
            .record_start
            .map(|t| t.elapsed().as_millis() as i64)
            .unwrap_or(0);
        // Prefer the shell-reported voiced_ms (energy VAD over the
        // drained samples) over wall-clock for the active-speech
        // metric so leaving the mic on after speaking doesn't
        // shrink Time Saved. Falls back to wall-clock when the
        // shell didn't push a voiced count (e.g. SwiftUI shell, or
        // a future shell that hasn't been updated yet).
        let duration_ms = s.voiced_ms_at_drain.take().unwrap_or(wall_clock_ms);
        s.transcript = text.to_string();
        s.confidence = confidence;
        s.status_message = "done — pasted to focused app".to_string();
        s.phase = PHASE_DONE;
        s.record_start = None;
        s.record_start_epoch_ms = None;
        (started_at_ms, duration_ms, wall_clock_ms)
    });
    // Outside the state lock — the injector spawns its own worker but
    // there's no reason to hold the dictation mutex across the call.
    if let Some(inj) = INJECTOR.get() {
        inj.inject(text);
    }
    // Transcript history (TASK-95) is opt-in: with it off nothing is
    // written and the frontmost app is not even read. It runs after
    // the injector was called: that pastes from its own thread and a
    // paste does not move focus, so the read below names the app that
    // receives the text without holding the paste up. It runs before
    // the stats write, whose "changed" signal then also covers it.
    let history = crate::settings::current_history_settings();
    if let Some(dir) = crate::history::current_dir() {
        if history.enabled {
            // The mode is part of the entry only when it shaped the
            // text: a custom mode whose answer was used. The built-in
            // mode is never written, also when the snapshot names it
            // (an app default).
            let (original, mode) = with_state(|s| {
                let custom = s.refine.status == "refined" && !s.refine.mode_builtin;
                let mode = if custom { s.refine.mode.clone() } else { String::new() };
                (s.refine.raw_transcript.clone(), mode)
            });
            let (app_id, app_name) = delivered_to_app();
            let at_ms = if started_at_ms > 0 { started_at_ms } else { crate::history::now_ms() };
            let entry = crate::history::NewEntry {
                at_ms,
                duration_ms,
                text,
                original: Some(original.as_str()),
                app_id: &app_id,
                app_name: &app_name,
                mode: &mode,
                kind: "",
                answer: None,
                screen: false,
            };
            if let Err(e) = crate::history::append(&dir, &entry) {
                eprintln!("[history] could not save the dictation: {e}");
            }
        }
        // Retention applies with history on or off.
        if let Err(e) = crate::history::prune(&dir, &history, crate::history::now_ms()) {
            eprintln!("[history] prune failed: {e}");
        }
    }
    // Stats write happens after inject so a failing DB doesn't delay the
    // paste. Stats writer no-ops on empty text and never panics or
    // mutates dictation phase, so a failure here cannot stall the
    // state machine in PHASE_TRANSCRIBING / push it to PHASE_ERROR.
    if let Some(store) = crate::stats::store() {
        crate::stats::record_dictation(
            store,
            started_at_ms,
            duration_ms,
            wall_clock_ms,
            text,
        );
    }
}

/// Id and name of the app the transcript was just delivered to, for
/// the history entry. Empty when it is not known, or when one of
/// OpenWhisper's own windows has focus (then the text only went to
/// the clipboard).
#[cfg(feature = "focus")]
fn delivered_to_app() -> (String, String) {
    let focus = crate::focus::current();
    if focus.is_self {
        return Default::default();
    }
    (focus.app_id, focus.app_name)
}

#[cfg(not(feature = "focus"))]
fn delivered_to_app() -> (String, String) {
    Default::default()
}

/// Implemented by the shell (Tauri's `TauriInjector`) and registered once
/// at boot via [`set_injector`]. Core calls it from
/// [`dictation_deliver_transcript`] so the paste flow lives in core
/// orchestration even though the OS surface for synthesizing keystrokes is
/// shell-side.
///
/// Mac SwiftUI shipped shell does NOT register an injector — it owns its
/// own paste flow in `TextInjector.swift`. With no injector registered the
/// dispatch in `dictation_deliver_transcript` no-ops, so the SwiftUI app is
/// unaffected by this hook.
pub trait Injector: Send + Sync {
    fn inject(&self, text: &str);
}

static INJECTOR: OnceLock<Box<dyn Injector>> = OnceLock::new();

/// Register the injector. First call wins; subsequent calls are silently
/// ignored (single-process app, single shell — no reason to swap mid-run).
pub fn set_injector(injector: Box<dyn Injector>) {
    let _ = INJECTOR.set(injector);
}

/// The recording or its transcription failed: ERROR, with `message`.
///
/// When the recording was for an ask, that ask ends here too, in its
/// own error phase: nothing will ever submit it. This is the one
/// place every failure passes (the recognizer that fails to load
/// while the person talks, the stop pipeline, a shell's own errors),
/// so no path can leave an ask on "listening".
pub fn dictation_deliver_error(message: &str) {
    IS_RECORDING.store(false, Ordering::Relaxed);
    let (kind, ask_id) = with_state(|s| {
        s.error_message = message.to_string();
        s.status_message = message.to_string();
        s.phase = PHASE_ERROR;
        s.record_start = None;
        s.record_start_epoch_ms = None;
        s.voiced_ms_at_drain = None;
        s.download_bytes_done = 0;
        s.download_bytes_total = 0;
        (s.kind, s.ask_id)
    });
    ask_fail(kind, ask_id, message);
    crate::crashes::event_buffer::push_event(
        "Error",
        serde_json::json!({ "message": message }),
    );
}

/// Platform-glue dependencies the dictation orchestration needs from the
/// shell:
///
/// - mic authorization (Mac AVCaptureDevice TCC; trivially true on
///   Windows / Linux until they grow one)
/// - the media-gate pause / resume pair (the shell wraps
///   [`crate::media_gate`] with its registered platform controller)
/// - a worker-thread spawn shim (the shell owns thread naming + thread
///   panics, so we hand a named closure across the boundary instead of
///   pulling `std::thread` into core orchestration paths)
///
/// Lives under `feature = "recognizer"` because the run_* fns that consume
/// this trait call into the recognizer subsystem; the SwiftUI shell drives
/// its own dictation loop in Swift and never compiles this surface.
#[cfg(feature = "recognizer")]
pub trait DictationEnv {
    fn mic_authorized(&self) -> bool;
    fn pause_audio(&self);
    fn resume_audio(&self);
    /// Spawn `f` on a named worker thread. The shell owns the spawn so it
    /// can pick its own naming convention + decide how to handle panics
    /// from the closure (typically: let them propagate to the panic hook
    /// that writes a crash file).
    fn spawn(&self, name: &'static str, f: Box<dyn FnOnce() + Send + 'static>);
}

/// Run a user-initiated dictation toggle. Implements the five-step
/// orchestration: mic-auth gate, request_toggle, mark_loading_model +
/// background recognizer load, pause-gate, audio_start_capture,
/// mark_capture_started; stop branch: stop_capture,
/// mark_transcribing_pending, spawn stop pipeline, resume-gate.
///
/// Returns `Err` only when `audio_start_capture` fails — every other
/// branch is fire-and-forget (mark_* / deliver_error path).
#[cfg(feature = "recognizer")]
pub fn run_toggle<E: DictationEnv>(env: &E) -> Result<(), String> {
    run_toggle_kind(env, SessionKind::Dictation)
}

/// [`run_toggle`] for a recording of the given kind: what the ask key
/// calls with [`SessionKind::Ask`]. While a recording is running, the
/// toggle stops it whatever `kind` says, and the recording is handled
/// as the kind it was started as.
///
/// An ask recording is only started while Ask is switched on (and
/// compiled in). Otherwise the toggle does nothing: the words must
/// not end up typed as a dictation.
#[cfg(feature = "recognizer")]
pub fn run_toggle_kind<E: DictationEnv>(env: &E, kind: SessionKind) -> Result<(), String> {
    // Gate: starting a recording without mic authorization would flip the
    // phase machine to RECORDING, fail at audio_start_capture, then
    // bounce back to ERROR — confusing UX. The mic-banner already tells
    // the user how to fix this; refuse to leave idle until it's
    // resolved. Only gate on the begin transition; stop / cancel must
    // always be allowed in case mic is revoked mid-recording.
    if !is_recording() && !env.mic_authorized() {
        crate::verbose_log!(
            "[ow.dictation] toggle blocked: mic not authorized; staying idle"
        );
        return Ok(());
    }
    if kind == SessionKind::Ask && !is_recording() && !ask_enabled() {
        crate::verbose_log!("[ow.dictation] ask toggle ignored: Ask is switched off");
        return Ok(());
    }
    let action = dictation_request_toggle_kind(kind);
    match action {
        TOGGLE_BEGIN_RECORDING => {
            let t_start = std::time::Instant::now();
            // Kick off model load lazily on first record so the UI's
            // "loading model" phase reflects real work. ensure_loaded is
            // idempotent — subsequent toggles short-circuit.
            dictation_mark_loading_model();
            env.spawn(
                "openwhisper-recognizer-load",
                Box::new(|| {
                    if let Err(e) = crate::recognizer::recognizer_ensure_loaded() {
                        dictation_deliver_error(&format!("recognizer load failed: {e}"));
                    }
                }),
            );
            // Fade out + pause other apps' audio BEFORE opening the
            // mic. See `media_gate::pause` doc.
            env.pause_audio();
            let pause_ms = t_start.elapsed().as_millis();
            crate::audio::audio_start_capture()?;
            dictation_mark_capture_started();
            // How long the hotkey press took to become a live mic.
            // Refine is not in this number: it warms up on its own
            // thread, started on the next line.
            crate::verbose_log!(
                "[ow.dictation] start pause_audio_ms={pause_ms} mic_live_ms={}",
                t_start.elapsed().as_millis()
            );
            // Off the hotkey thread: it reads the dictionary, may read
            // the keychain, and spawns a process.
            match kind {
                SessionKind::Ask => ask_begin(env),
                SessionKind::Dictation => refine_warm(env),
            }
        }
        TOGGLE_STOP_RECORDING => {
            // Stop capture is cheap (cpal stream teardown). The expensive
            // bit — sinc resampling the buffer to 16 kHz — used to run
            // here on the hotkey thread, blocking the phase transition
            // and freezing the UI on "recording" for ~1–2 s on Windows.
            // Now we flip phase optimistically and let the worker thread
            // drain + resample as the first step of transcription.
            crate::audio::audio_stop_capture();
            dictation_mark_transcribing_pending();
            env.spawn(
                "openwhisper-stop-pipeline",
                Box::new(run_stop_pipeline),
            );
            env.resume_audio();
        }
        _ => {}
    }
    Ok(())
}

/// Run a user-initiated cancel: what the cancel key does. It is for
/// one thing at a time.
///
/// - **A recording is in flight.** Stops audio capture, drains the
///   buffer (samples are discarded), flips the phase machine to IDLE
///   without emitting a transcript, drops what was warmed for it and
///   resumes paused media. When the recording was for an ask, that
///   ask ends with it. An ask that is still answering in its panel
///   while a *dictation* is cancelled is left alone.
/// - **Nothing is being recorded.** Then the key is for an ask that
///   is being transcribed or answered, and only for that: the
///   recorder, the sample buffer and Refine are not touched, so a
///   dictation that is being transcribed at that moment keeps its
///   samples and its warmed Refine backend. A finished answer is left
///   alone.
///
/// The answer panel has its own two entries, [`run_cancel_ask`] and
/// [`run_dismiss_ask`].
///
/// Returns `true` if a recording was actually cancelled or a running
/// ask was stopped.
#[cfg(feature = "recognizer")]
pub fn run_cancel<E: DictationEnv>(env: &E) -> bool {
    if !is_recording() {
        return ask_cancel(None);
    }
    crate::audio::audio_stop_capture();
    let _ = crate::audio::audio_drain_samples();
    let stopped = cancel_recording();
    env.resume_audio();
    stopped
}

/// The state half of cancelling the recording in flight: the phase
/// machine, what was warmed for the recording, and the ask it was
/// for.
#[cfg(feature = "recognizer")]
fn cancel_recording() -> bool {
    let (kind, ask_id) = with_state(|s| (s.kind, s.ask_id));
    let cancelled = dictation_request_cancel();
    refine_discard();
    let ask_stopped = match kind {
        // A dictation was cancelled. An ask that is still answering
        // in its panel is none of this key press's business.
        SessionKind::Dictation => false,
        // The ask this recording was for, and no other.
        SessionKind::Ask => ask_cancel(Some(ask_id)),
    };
    cancelled || ask_stopped
}

/// Begin an audio-preview session (Settings → Audio mic meter). Mutually
/// exclusive with an active recording — the hotkey path can't slip in
/// between this check and start_preview because audio_start_capture
/// itself stops the preview, but if a recording IS already in flight we
/// return the precise reason here.
///
/// Same audio-ducking semantics as a real recording: pause other apps'
/// audio BEFORE opening the mic so BT headphones don't sit in HFP/mono
/// with music still mid-playback.
#[cfg(feature = "recognizer")]
pub fn run_preview_start<E: DictationEnv>(env: &E) -> Result<(), String> {
    if is_recording() {
        return Err("recording in progress".into());
    }
    env.pause_audio();
    crate::audio::audio_preview_start()
}

/// End the audio-preview session and resume any paused media.
#[cfg(feature = "recognizer")]
pub fn run_preview_stop<E: DictationEnv>(env: &E) {
    crate::audio::audio_preview_stop();
    env.resume_audio();
}

/// Drain the captured buffer (downmix + sinc resample to 16 kHz) and run
/// the recognizer. Designed to run inside a worker thread spawned by the
/// shell after [`run_toggle`] flips phase to TRANSCRIBING — the hotkey
/// thread has already redrawn the UI by the time this starts.
///
/// Mac path = FluidAudio + ANE; Win path = sherpa-onnx / ort + CPU. See
/// `recognizer/mod.rs` for the OS-conditional impl.
#[cfg(feature = "recognizer")]
pub fn run_stop_pipeline() {
    // The kind the recording was started as, not the key that
    // stopped it, and the ask it was for. Read once, here: nothing
    // can start a new recording while this one is TRANSCRIBING, but
    // from DONE or ERROR on, a new ask can be on its way.
    let (kind, ask_id) = with_state(|s| (s.kind, s.ask_id));
    // The recording could not be transcribed. For an ask, the error
    // delivery ends the ask too ([`dictation_deliver_error`]).
    let fail = |message: &str| {
        if kind == SessionKind::Dictation {
            refine_discard();
        }
        dictation_deliver_error(message);
    };
    let t_drain = std::time::Instant::now();
    let samples = crate::audio::audio_drain_samples();
    let count = samples.len() as u64;
    let drain_ms = t_drain.elapsed().as_millis();
    // Energy-VAD over the drained samples — the stats writer reads this
    // off dictation state and uses it as the recording's effective
    // duration_ms in place of wall-clock, so silence at the tail
    // doesn't penalize Time Saved. Sample rate is the constant 16 kHz
    // core resamples to before exposing samples
    // (audio::TARGET_SAMPLE_RATE).
    let voiced_ms = crate::audio::estimate_voiced_ms(&samples, 16_000);
    dictation_set_voiced_ms(voiced_ms);
    // Empty mic → mark_capture_stopped flips phase back to DONE with
    // "no audio captured". Populated → updates sample_count and
    // reaffirms TRANSCRIBING (no-op vs the optimistic flip).
    dictation_mark_capture_stopped(count);
    if count == 0 {
        crate::verbose_log!("[ow.dictation] stop empty drain_ms={drain_ms}");
        // Nothing was said: no error, and what was started for the
        // recording goes. The phase is DONE by now, so the ask is
        // closed by its id: a new one may have started already.
        match kind {
            SessionKind::Ask => ask_dismiss(Some(ask_id)),
            SessionKind::Dictation => refine_discard(),
        }
        return;
    }
    // Defensive: recognizer_transcribe requires the engine to be
    // initialized. Loader was kicked off at recording start, but a slow
    // first-load might still be in flight — re-call ensure_loaded so we
    // block until it's ready.
    let t_load = std::time::Instant::now();
    if let Err(e) = crate::recognizer::recognizer_ensure_loaded() {
        fail(&format!("recognizer load: {e}"));
        return;
    }
    let load_ms = t_load.elapsed().as_millis();
    let t_tx = std::time::Instant::now();
    match crate::recognizer::recognizer_transcribe(&samples) {
        Ok(res) => {
            let transcribe_ms = t_tx.elapsed().as_millis();
            if kind == SessionKind::Ask {
                crate::verbose_log!(
                    "[ow.dictation] stop (ask) drain_ms={drain_ms} ensure_loaded_ms={load_ms} \
                     transcribe_ms={transcribe_ms} samples={count} chars={}",
                    res.text.len()
                );
                // Blocks until the answer is complete; this worker
                // thread has nothing else to do.
                ask_stage(&res.text);
                return;
            }
            let (cleaned, replacements) = crate::transcript::process_tracked(&res.text);
            // Before delivery, so the DONE tick already carries them.
            with_state(|s| s.replacements = replacements);
            // Refine sits here: after the dictionary, before the text
            // reaches the UI snapshot, the cursor and the clipboard.
            let t_refine = std::time::Instant::now();
            let text = refine_stage(&cleaned);
            let refine_ms = t_refine.elapsed().as_millis();
            crate::verbose_log!(
                "[ow.dictation] stop drain_ms={drain_ms} ensure_loaded_ms={load_ms} \
                 transcribe_ms={transcribe_ms} refine_ms={refine_ms} samples={count} \
                 chars={} confidence={:.2}",
                text.len(),
                res.confidence
            );
            dictation_deliver_transcript(&text, res.confidence);
        }
        Err(e) => fail(&format!("transcribe: {e}")),
    }
}

/// Background warmup of the recognizer at app boot. Cold-loading on
/// Windows takes ~2.5 s (sherpa-onnx + Parakeet int8); doing it on a
/// worker thread at boot means the in-line load inside
/// [`run_toggle`]'s begin branch becomes a no-op once this completes, so
/// the first Record click decodes at steady-state latency instead of
/// paying the wait. `recognizer_ensure_loaded` is idempotent, so a slow
/// warmup overlapping a fast first Record still yields the correct
/// result.
///
/// Phase ownership during warmup: flips dictation phase to LOADING_MODEL
/// on entry so the UI surfaces the boot-time download (~487 MB on first
/// run). On success, hands control back to IDLE via
/// `dictation_mark_loaded` — that helper only flips IDLE if phase is
/// still LOADING_MODEL, so a user-driven recording start that overlaps
/// with the warmup completion isn't clobbered. On failure, routes
/// through `deliver_error` so the recognizer banner picks it up.
#[cfg(feature = "recognizer")]
pub fn run_warmup() {
    dictation_mark_loading_model();
    match crate::recognizer::recognizer_ensure_loaded() {
        Ok(()) => dictation_mark_loaded(),
        Err(e) => {
            eprintln!("[warmup] recognizer load failed: {e}");
            dictation_deliver_error(&format!("recognizer load: {e}"));
        }
    }
}

// Tests mutate the global STATE singleton. Serialize them with a
// dedicated test-only mutex so parallel execution doesn't corrupt
// state between assertions. At module level so tests elsewhere that
// depend on what a dictation leaves behind (the history settings it
// switches) can hold it too.
#[cfg(test)]
pub(crate) static TEST_LOCK: Mutex<()> = Mutex::new(());

#[cfg(test)]
mod tests {
    use std::sync::MutexGuard;

    use super::*;

    fn start() -> MutexGuard<'static, ()> {
        let guard = TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        with_state(|s| *s = State::new());
        IS_RECORDING.store(false, Ordering::Relaxed);
        guard
    }

    #[test]
    fn toggle_from_idle_begins_recording() {
        let _lock = start();
        assert_eq!(dictation_request_toggle(), TOGGLE_BEGIN_RECORDING);
        dictation_mark_capture_started();
        let snap = dictation_snapshot();
        assert_eq!(snap.phase(), PHASE_RECORDING);
        assert!(snap.is_recording());
    }

    #[test]
    fn toggle_from_recording_stops() {
        let _lock = start();
        let _ = dictation_request_toggle();
        dictation_mark_capture_started();
        assert_eq!(dictation_request_toggle(), TOGGLE_STOP_RECORDING);
    }

    #[test]
    fn toggle_ignored_during_transcribing() {
        let _lock = start();
        let _ = dictation_request_toggle();
        dictation_mark_capture_started();
        dictation_mark_capture_stopped(1000);
        assert_eq!(dictation_request_toggle(), TOGGLE_IGNORE);
    }

    #[test]
    fn toggle_ignored_during_loading_model() {
        let _lock = start();
        let _ = dictation_request_toggle();
        dictation_mark_loading_model();
        assert_eq!(dictation_request_toggle(), TOGGLE_IGNORE);
    }

    #[test]
    fn cancel_only_while_recording() {
        let _lock = start();
        assert!(!dictation_request_cancel()); // idle → false
        let _ = dictation_request_toggle();
        dictation_mark_capture_started();
        assert!(dictation_request_cancel()); // recording → true
        assert_eq!(dictation_snapshot().phase(), PHASE_IDLE);
    }

    #[test]
    fn is_recording_mirror_tracks_phase() {
        let _lock = start();
        assert!(!is_recording());
        let _ = dictation_request_toggle();
        // Toggle alone doesn't flip the mirror — capture_started does.
        assert!(!is_recording());
        dictation_mark_capture_started();
        assert!(is_recording());
        let _ = dictation_request_cancel();
        assert!(!is_recording());

        // Stop path: start → transcribing_pending clears mirror.
        let _ = dictation_request_toggle();
        dictation_mark_capture_started();
        assert!(is_recording());
        dictation_mark_transcribing_pending();
        assert!(!is_recording());

        // Error path also clears.
        let _ = dictation_request_toggle();
        dictation_mark_capture_started();
        assert!(is_recording());
        dictation_deliver_error("boom");
        assert!(!is_recording());
    }

    #[test]
    fn empty_samples_skip_transcribe() {
        let _lock = start();
        let _ = dictation_request_toggle();
        dictation_mark_capture_started();
        dictation_mark_capture_stopped(0);
        assert_eq!(dictation_snapshot().phase(), PHASE_DONE);
    }

    #[test]
    fn deliver_transcript_sets_fields() {
        let _lock = start();
        let _ = dictation_request_toggle();
        dictation_mark_capture_started();
        dictation_mark_capture_stopped(16000);
        dictation_deliver_transcript("hello world", 0.92);
        let snap = dictation_snapshot();
        assert_eq!(snap.phase(), PHASE_DONE);
        assert_eq!(snap.transcript(), "hello world");
        assert!((snap.confidence() - 0.92).abs() < 1e-6);
    }

    #[test]
    fn deliver_error_transitions_and_recovers() {
        let _lock = start();
        let _ = dictation_request_toggle();
        dictation_deliver_error("mic start failed");
        assert_eq!(dictation_snapshot().phase(), PHASE_ERROR);
        // User can toggle from Error to start fresh.
        assert_eq!(dictation_request_toggle(), TOGGLE_BEGIN_RECORDING);
    }

    #[test]
    fn try_snapshot_returns_none_when_idle() {
        let _lock = start();
        // STATE is initialized by `start()` via `with_state` — phase=IDLE.
        // Crash outside a dictation should produce no recording_state.
        assert!(try_snapshot_for_crash().is_none());
    }

    #[test]
    fn try_snapshot_returns_some_during_recording() {
        let _lock = start();
        let _ = dictation_request_toggle();
        dictation_mark_capture_started();
        let snap = try_snapshot_for_crash().expect("recording snapshot");
        assert_eq!(snap.status_message_at_crash, "recording — tap again to stop");
        assert_eq!(snap.samples_captured, 0);
        assert!(snap.model_kind.is_none());
        assert!(snap.device_id_hash.is_none());
    }

    #[test]
    fn try_snapshot_returns_some_during_transcribing() {
        let _lock = start();
        let _ = dictation_request_toggle();
        dictation_mark_capture_started();
        dictation_mark_capture_stopped(16000);
        let snap = try_snapshot_for_crash().expect("transcribing snapshot");
        assert_eq!(snap.samples_captured, 16000);
    }

    #[test]
    fn refine_result_rides_the_snapshot_and_clears_with_the_transcript() {
        let _lock = start();
        let _ = dictation_request_toggle();
        dictation_mark_capture_started();
        dictation_mark_capture_stopped(16000);
        dictation_mark_refining();
        assert_eq!(dictation_snapshot().status_message(), "refining…");
        assert_eq!(dictation_snapshot().phase(), PHASE_TRANSCRIBING);
        dictation_set_refine_result(RefineInfo {
            raw_transcript: "hello world ".into(),
            status: "refined".into(),
            detail: String::new(),
            instruction: String::new(),
            backend: "claude-cli".into(),
            elapsed_ms: 740,
            mode: "Formal".into(),
            mode_builtin: false,
            mode_reason: "app-default".into(),
            mode_app: "Mail".into(),
        });
        dictation_deliver_transcript("Hello, world. ", 0.9);
        let snap = dictation_snapshot();
        assert_eq!(snap.transcript(), "Hello, world. ");
        assert_eq!(snap.refine().raw_transcript, "hello world ");
        assert_eq!(snap.refine().status, "refined");
        assert_eq!(snap.refine().elapsed_ms, 740);
        assert_eq!(snap.refine().mode, "Formal");
        assert_eq!(snap.refine().mode_reason, "app-default");
        assert_eq!(snap.refine().mode_app, "Mail");
        // Next recording starts clean.
        let _ = dictation_request_toggle();
        assert_eq!(dictation_snapshot().refine(), RefineInfo::default());
    }

    /// Deliver one dictation with history on, writing to `dir`, and
    /// return the stored entry. `refine` is what the Refine stage
    /// reported, if it ran.
    fn deliver_with_history(dir: &std::path::Path, refine: Option<RefineInfo>) -> crate::history::HistoryEntry {
        crate::settings::set_history_settings_cache(crate::settings::HistorySettings {
            enabled: true,
            retention_days: 0,
            folder: Some(dir.to_string_lossy().into_owned()),
        });
        let _ = dictation_request_toggle();
        dictation_mark_capture_started();
        dictation_mark_capture_stopped(16000);
        if let Some(info) = refine {
            dictation_set_refine_result(info);
        }
        dictation_deliver_transcript("Could you please send the price list? ", 0.9);
        crate::settings::set_history_settings_cache(crate::settings::HistorySettings::default());
        let page = crate::history::list(dir, "", 10, 0);
        page.entries.into_iter().next().expect("the dictation was stored")
    }

    fn refined_in(mode: &str, status: &str) -> RefineInfo {
        RefineInfo {
            raw_transcript: "hey oskar send me the price list ".into(),
            status: status.into(),
            backend: "claude-cli".into(),
            mode: mode.into(),
            mode_reason: "selected".into(),
            ..RefineInfo::default()
        }
    }

    #[test]
    fn history_entry_names_the_custom_mode_that_rewrote_the_text() {
        let _lock = start();
        let dir = tempfile::tempdir().unwrap();
        let entry = deliver_with_history(dir.path(), Some(refined_in("Formal", "refined")));
        assert_eq!(entry.mode.as_deref(), Some("Formal"));
        assert_eq!(entry.text, "Could you please send the price list?");
        assert_eq!(entry.original.as_deref(), Some("hey oskar send me the price list"));
    }

    #[test]
    fn history_entry_has_no_mode_for_dictation_or_when_refine_did_not_change_the_text() {
        let _lock = start();
        // The built-in mode: `mode` is empty on the snapshot.
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(deliver_with_history(dir.path(), Some(refined_in("", "refined"))).mode, None);
        // A custom mode whose answer was not used: the text is what
        // was said, so the mode did not shape it.
        for status in ["timeout", "rejected", "unavailable", "failed", "skipped-short"] {
            let dir = tempfile::tempdir().unwrap();
            with_state(|s| *s = State::new());
            assert_eq!(deliver_with_history(dir.path(), Some(refined_in("Formal", status))).mode, None, "{status}");
        }
        // Refine off: nothing ran.
        let dir = tempfile::tempdir().unwrap();
        with_state(|s| *s = State::new());
        assert_eq!(deliver_with_history(dir.path(), None).mode, None);
    }

    #[cfg(feature = "refine")]
    fn outcome_in(mode: crate::refine::ModeUse) -> crate::refine::RefineOutcome {
        crate::refine::RefineOutcome {
            text: "Could you please send the price list? ".into(),
            raw: "hey oskar send me the price list ".into(),
            status: crate::refine::RefineStatus::Refined,
            detail: String::new(),
            instruction: String::new(),
            backend: "claude-cli".into(),
            model: "haiku".into(),
            effort: "low".into(),
            elapsed_ms: 640,
            warm_lead_ms: None,
            usage: None,
            mode,
        }
    }

    #[cfg(feature = "refine")]
    #[test]
    fn the_snapshot_names_dictation_when_an_app_default_put_it_over_the_selected_mode() {
        use crate::refine::ModeUse;
        // Dictation because the app says so: named, with the app.
        let by_app = ModeUse { reason: "app-default".into(), app_name: "Terminal".into(), ..ModeUse::default() };
        let info = RefineInfo::from_outcome(&outcome_in(by_app)).unwrap();
        assert_eq!(info.mode, "Dictation");
        assert!(info.mode_builtin);
        assert_eq!((info.mode_reason.as_str(), info.mode_app.as_str()), ("app-default", "Terminal"));
        assert_eq!(info.raw_transcript, "hey oskar send me the price list ");
        assert_eq!((info.status.as_str(), info.elapsed_ms), ("refined", 640));

        // Dictation as the selected mode: the plain case, no label.
        let info = RefineInfo::from_outcome(&outcome_in(ModeUse::default())).unwrap();
        assert_eq!((info.mode.as_str(), info.mode_builtin), ("", true));
        assert_eq!(info.mode_reason, "selected");

        // A custom mode is named either way.
        for reason in ["selected", "app-default"] {
            let custom = ModeUse {
                id: "formal".into(),
                name: "Formal".into(),
                reason: reason.into(),
                app_name: if reason == "selected" { String::new() } else { "Mail".into() },
            };
            let info = RefineInfo::from_outcome(&outcome_in(custom)).unwrap();
            assert_eq!((info.mode.as_str(), info.mode_builtin), ("Formal", false), "{reason}");
            assert_eq!(info.mode_reason, reason);
        }

        // Refine off: nothing to tell.
        assert_eq!(RefineInfo::from_outcome(&crate::refine::RefineOutcome::disabled("hello ")), None);
    }

    #[cfg(feature = "refine")]
    #[test]
    fn history_never_records_the_built_in_mode_even_when_the_snapshot_names_it() {
        let _lock = start();
        let by_app = crate::refine::ModeUse {
            reason: "app-default".into(),
            app_name: "Terminal".into(),
            ..crate::refine::ModeUse::default()
        };
        let info = RefineInfo::from_outcome(&outcome_in(by_app)).unwrap();
        assert_eq!(info.mode, "Dictation");
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(deliver_with_history(dir.path(), Some(info)).mode, None);
    }

    #[test]
    fn refining_status_only_while_transcribing() {
        let _lock = start();
        dictation_mark_refining();
        assert_ne!(dictation_snapshot().status_message(), "refining…");
    }

    #[test]
    fn new_session_clears_previous_transcript() {
        let _lock = start();
        let _ = dictation_request_toggle();
        dictation_mark_capture_started();
        dictation_mark_capture_stopped(16000);
        dictation_deliver_transcript("first run", 0.8);
        assert_eq!(dictation_snapshot().transcript(), "first run");
        let _ = dictation_request_toggle();
        assert_eq!(dictation_snapshot().transcript(), "");
    }

    // --- the ask kind (TASK-97) ---------------------------------------

    #[test]
    fn an_ask_toggle_keeps_the_last_dictation_on_the_snapshot() {
        let _lock = start();
        // A dictation, with everything it leaves behind.
        let _ = dictation_request_toggle();
        dictation_mark_capture_started();
        dictation_mark_capture_stopped(16000);
        dictation_set_refine_result(refined_in("Formal", "refined"));
        dictation_deliver_transcript("Could you please send the price list? ", 0.9);
        assert_eq!(dictation_snapshot().session_kind(), SessionKind::Dictation);
        let before = dictation_snapshot();

        // Starting an ask recording clears none of it.
        assert_eq!(dictation_request_toggle_kind(SessionKind::Ask), TOGGLE_BEGIN_RECORDING);
        dictation_mark_capture_started();
        let during = dictation_snapshot();
        assert_eq!(during.session_kind(), SessionKind::Ask);
        assert!(during.is_recording());
        assert_eq!(during.transcript(), before.transcript());
        assert_eq!(during.refine(), before.refine());
        assert!((during.confidence() - 0.9).abs() < 1e-6);

        // Cancelling it does not either, and the next dictation
        // starts clean as always.
        assert!(dictation_request_cancel());
        assert_eq!(dictation_snapshot().transcript(), before.transcript());
        assert_eq!(dictation_snapshot().refine(), before.refine());
        assert_eq!(dictation_request_toggle(), TOGGLE_BEGIN_RECORDING);
        assert_eq!(dictation_snapshot().session_kind(), SessionKind::Dictation);
        assert_eq!(dictation_snapshot().transcript(), "");
        assert_eq!(dictation_snapshot().refine(), RefineInfo::default());
    }

    #[test]
    fn a_recording_keeps_the_kind_it_started_with_whichever_key_stops_it() {
        let _lock = start();
        // Started as an ask, stopped with the dictation key.
        assert_eq!(dictation_request_toggle_kind(SessionKind::Ask), TOGGLE_BEGIN_RECORDING);
        dictation_mark_capture_started();
        assert_eq!(dictation_request_toggle(), TOGGLE_STOP_RECORDING);
        assert_eq!(dictation_snapshot().session_kind(), SessionKind::Ask);
        dictation_mark_transcribing_pending();
        assert_eq!(dictation_snapshot().session_kind(), SessionKind::Ask);
        // Neither key does anything while it is transcribed.
        assert_eq!(dictation_request_toggle_kind(SessionKind::Ask), TOGGLE_IGNORE);
        assert_eq!(dictation_request_toggle(), TOGGLE_IGNORE);

        // Started as a dictation, stopped with the ask key.
        with_state(|s| *s = State::new());
        assert_eq!(dictation_request_toggle(), TOGGLE_BEGIN_RECORDING);
        dictation_mark_capture_started();
        assert_eq!(dictation_request_toggle_kind(SessionKind::Ask), TOGGLE_STOP_RECORDING);
        assert_eq!(dictation_snapshot().session_kind(), SessionKind::Dictation);

        assert_eq!(SessionKind::Ask.as_str(), "ask");
        assert_eq!(serde_json::to_string(&SessionKind::Dictation).unwrap(), "\"dictation\"");
        assert_eq!(SessionKind::default(), SessionKind::Dictation);
    }

    #[cfg(all(feature = "recognizer", feature = "ask"))]
    mod ask_kind {
        use std::sync::atomic::AtomicUsize;
        use std::sync::{Arc, Mutex};

        use super::super::*;
        use super::{refined_in, start};
        use crate::ask::test_support::{Calls, Stub, scope};
        use crate::ask::{self, AskError, AskEvent, AskPhase};
        use crate::settings::{AskSettings, HistorySettings};

        /// How often core handed text to the injector, in the whole
        /// test process. Every test that reads it holds the dictation
        /// test lock, and so does everything that delivers a
        /// transcript.
        static INJECTED: AtomicUsize = AtomicUsize::new(0);

        struct CountingInjector;

        impl Injector for CountingInjector {
            fn inject(&self, _text: &str) {
                INJECTED.fetch_add(1, Ordering::SeqCst);
            }
        }

        fn injected() -> usize {
            // First call wins; nothing else in the tests registers one.
            set_injector(Box::new(CountingInjector));
            INJECTED.load(Ordering::SeqCst)
        }

        /// A shell that runs spawned work on the spot and counts it.
        #[derive(Default)]
        struct InlineEnv {
            spawned: Mutex<Vec<&'static str>>,
            /// How often paused media was resumed.
            resumed: AtomicUsize,
        }

        impl DictationEnv for InlineEnv {
            fn mic_authorized(&self) -> bool {
                true
            }
            fn pause_audio(&self) {}
            fn resume_audio(&self) {
                self.resumed.fetch_add(1, Ordering::SeqCst);
            }
            fn spawn(&self, name: &'static str, f: Box<dyn FnOnce() + Send + 'static>) {
                self.spawned.lock().unwrap().push(name);
                f();
            }
        }

        /// A factory whose backends say "Partial", then wait until
        /// they are cancelled (or give up after five seconds).
        fn waiting(
            calls: &Arc<Mutex<Calls>>,
        ) -> impl Fn(&ask::AskPlan) -> Box<dyn ask::AskBackend> + Send + Sync + use<> {
            let calls = calls.clone();
            move |_| {
                Box::new(Stub::new(calls.clone(), |_, sink, cancel| {
                    sink(AskEvent::Text("Partial".into()));
                    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
                    while !cancel.is_cancelled() && std::time::Instant::now() < deadline {
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                    Err(AskError::Cancelled)
                }))
            }
        }

        /// Ask a typed question on a thread of its own and wait
        /// until its answer is streaming in.
        fn ask_in_the_background() -> std::thread::JoinHandle<Result<ask::AskSnapshot, AskError>> {
            let worker = std::thread::spawn(|| ask::submit("tell me a long story"));
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while ask::snapshot().phase != AskPhase::Answering {
                assert!(std::time::Instant::now() < deadline, "the ask never started answering");
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            worker
        }

        fn on() -> AskSettings {
            AskSettings { enabled: true, ..AskSettings::default() }
        }

        fn dictation_rows(store: &crate::store::Store) -> i64 {
            store
                .with_conn(|c| Ok(c.query_row("SELECT COUNT(*) FROM dictations", [], |r| r.get(0))?))
                .unwrap()
        }

        /// A dictation that leaves a transcript, dictionary fixes and
        /// a Refine result on the snapshot.
        fn dictate() {
            let _ = dictation_request_toggle();
            dictation_mark_capture_started();
            dictation_mark_capture_stopped(16000);
            with_state(|s| {
                s.replacements = vec![crate::dictionary::Replacement {
                    from: "bren holt".into(),
                    to: "Brennholt".into(),
                }];
            });
            dictation_set_refine_result(refined_in("Formal", "refined"));
            dictation_deliver_transcript("Brennholt sent the price list. ", 0.9);
        }

        /// The recording part of an ask, as `run_toggle_kind` and the
        /// stop key do it, without a microphone.
        fn record_ask(env: &InlineEnv) {
            assert_eq!(dictation_request_toggle_kind(SessionKind::Ask), TOGGLE_BEGIN_RECORDING);
            dictation_mark_capture_started();
            ask_begin(env);
            assert_eq!(dictation_request_toggle(), TOGGLE_STOP_RECORDING);
            dictation_mark_transcribing_pending();
            dictation_set_voiced_ms(1800);
            dictation_mark_capture_stopped(16000);
        }

        #[test]
        fn an_ask_recording_reaches_the_ask_and_nothing_a_dictation_reaches() {
            let _lock = start();
            let store = crate::ask::test_support::store();
            let calls = Arc::new(Mutex::new(Calls::default()));
            let _scope = scope(on(), {
                let calls = calls.clone();
                move |_| Box::new(Stub::saying(calls.clone(), "Four."))
            });
            // Refine is on, with a backend that counts: an ask must
            // still not go through it.
            let refined = Arc::new(Mutex::new(crate::refine::test_support::Calls::default()));
            crate::refine::test_support::set_factory(Some(Arc::new({
                let refined = refined.clone();
                move |_: &crate::settings::RefineSettings| -> Box<dyn crate::refine::Refiner> {
                    Box::new(crate::refine::test_support::Stub::new(refined.clone(), |t| Ok(t.to_string())))
                }
            })));
            crate::settings::set_refine_settings_cache(crate::settings::RefineSettings {
                enabled: true,
                ..crate::settings::RefineSettings::default()
            });
            assert!(crate::refine::will_call_backend("one two three four five"));
            let dir = tempfile::tempdir().unwrap();
            crate::settings::set_history_settings_cache(HistorySettings {
                enabled: true,
                retention_days: 0,
                folder: Some(dir.path().to_string_lossy().into_owned()),
            });

            dictate();
            let before = dictation_snapshot();
            let injected_before = injected();
            let rows_before = dictation_rows(store);
            assert_eq!(crate::history::list(dir.path(), "", 10, 0).stored, 1);

            let env = InlineEnv::default();
            record_ask(&env);
            // The ask's backend warmed up in place of Refine's.
            assert_eq!(*env.spawned.lock().unwrap(), ["openwhisper-ask-warm"]);
            assert_eq!(calls.lock().unwrap().warmed, 1);
            assert_eq!(ask::snapshot().phase, AskPhase::Listening);
            assert!(cancel_would_act());

            // The recognizer's words, as they come: the rule pass and
            // the dictionary run on them like on a dictation.
            ask_stage("um what is two plus two");

            let after = dictation_snapshot();
            assert_eq!(after.phase(), PHASE_DONE);
            assert_eq!(after.session_kind(), SessionKind::Ask);
            assert!(!after.is_recording());
            // The last dictation is still what the snapshot holds.
            assert_eq!(after.transcript(), "Brennholt sent the price list. ");
            assert_eq!(after.transcript(), before.transcript());
            assert_eq!(after.replacements(), before.replacements());
            assert_eq!(after.refine(), before.refine());
            assert!((after.confidence() - before.confidence()).abs() < 1e-6);

            // The question went to the ask, cleaned, and was answered.
            let asked = ask::snapshot();
            assert_eq!(asked.phase, AskPhase::Done);
            assert_eq!(asked.question, "what is two plus two");
            assert_eq!(asked.answer, "Four.");
            assert_eq!(calls.lock().unwrap().sent, ["<question>\nwhat is two plus two\n</question>"]);
            assert!(!cancel_would_act());

            // And to nothing else: not the injector, not the stats,
            // not Refine, not the dictation history.
            assert_eq!(injected(), injected_before, "the answer or the question was handed to the injector");
            assert_eq!(dictation_rows(store), rows_before, "an ask was counted as a dictation");
            {
                let refined = refined.lock().unwrap();
                assert_eq!(refined.warmed, 0, "Refine was warmed for an ask recording");
                assert!(refined.refined.is_empty(), "the question went through Refine");
            }
            let page = crate::history::list(dir.path(), "", 10, 0);
            assert_eq!(page.stored, 2);
            let entry = &page.entries[0];
            assert!(entry.is_ask());
            assert_eq!((entry.text.as_str(), entry.answer.as_deref()), ("what is two plus two", Some("Four.")));
            assert_eq!(entry.duration_ms, 1800);
            assert!(!page.entries[1].is_ask());

            crate::refine::test_support::set_factory(None);
            crate::settings::set_history_settings_cache(HistorySettings::default());
        }

        #[test]
        fn cancelling_an_ask_recording_ends_the_ask_and_keeps_the_last_dictation() {
            let _lock = start();
            let calls = Arc::new(Mutex::new(Calls::default()));
            let _scope = scope(on(), {
                let calls = calls.clone();
                move |_| Box::new(Stub::saying(calls.clone(), "Four."))
            });
            dictate();
            assert_eq!(dictation_request_toggle_kind(SessionKind::Ask), TOGGLE_BEGIN_RECORDING);
            dictation_mark_capture_started();
            ask_begin(&InlineEnv::default());
            assert_eq!(ask::snapshot().phase, AskPhase::Listening);

            // What `run_cancel` does, without the audio calls.
            assert!(cancel_recording());

            assert_eq!(ask::snapshot().phase, AskPhase::Idle);
            assert_eq!(dictation_snapshot().phase(), PHASE_IDLE);
            assert_eq!(dictation_snapshot().transcript(), "Brennholt sent the price list. ");
            assert_eq!(calls.lock().unwrap().dropped, 1, "the warmed backend is gone");
            assert!(calls.lock().unwrap().sent.is_empty());
            assert!(!cancel_would_act());
        }

        #[test]
        fn the_cancel_key_stops_an_ask_that_is_still_being_transcribed() {
            let _lock = start();
            let calls = Arc::new(Mutex::new(Calls::default()));
            let _scope = scope(on(), {
                let calls = calls.clone();
                move |_| Box::new(Stub::saying(calls.clone(), "Four."))
            });
            record_ask(&InlineEnv::default());
            // Nothing is recording, and the gate is still open.
            assert!(!is_recording());
            assert!(cancel_would_act());

            // Nothing is recording: the cancel key is for the ask.
            assert!(run_cancel(&InlineEnv::default()));
            assert_eq!(ask::snapshot().phase, AskPhase::Idle);

            // The transcript arrives after the cancel: it is dropped.
            ask_stage("what is two plus two");
            assert_eq!(ask::snapshot().phase, AskPhase::Idle);
            assert!(calls.lock().unwrap().sent.is_empty());
            assert_eq!(dictation_snapshot().phase(), PHASE_DONE);
            // Once nothing is in flight, the key is the app's again.
            assert!(!cancel_would_act());
            assert!(!run_cancel_ask(&InlineEnv::default(), None));
            assert!(!run_cancel(&InlineEnv::default()));
        }

        #[test]
        fn cancelling_a_dictation_leaves_a_running_ask_alone() {
            let _lock = start();
            let calls = Arc::new(Mutex::new(Calls::default()));
            let _scope = scope(on(), waiting(&calls));
            let worker = ask_in_the_background();

            // A dictation is started and cancelled meanwhile.
            assert_eq!(dictation_request_toggle(), TOGGLE_BEGIN_RECORDING);
            dictation_mark_capture_started();
            assert!(cancel_recording());
            assert_eq!(dictation_snapshot().phase(), PHASE_IDLE);
            assert_eq!(ask::snapshot().phase, AskPhase::Answering);

            // With nothing recording, the same key stops the ask.
            assert!(cancel_would_act());
            assert!(run_cancel(&InlineEnv::default()));
            assert_eq!(ask::snapshot().phase, AskPhase::Cancelled);
            assert_eq!(worker.join().unwrap().unwrap().phase, AskPhase::Cancelled);
            assert!(!cancel_would_act());
            // The panel's cancel leaves the cancelled ask to look at;
            // closing the panel takes it away.
            assert_eq!(ask::snapshot().answer, "Partial");
            run_dismiss_ask(&InlineEnv::default(), None);
            assert_eq!(ask::snapshot().phase, AskPhase::Idle);
            assert_eq!(ask::snapshot().answer, "");
        }

        #[test]
        fn cancelling_an_ask_leaves_a_dictation_that_is_being_transcribed_alone() {
            let _lock = start();
            let calls = Arc::new(Mutex::new(Calls::default()));
            let _scope = scope(on(), waiting(&calls));
            // Refine is on, with a backend that counts.
            let refined = Arc::new(Mutex::new(crate::refine::test_support::Calls::default()));
            crate::refine::test_support::set_factory(Some(Arc::new({
                let refined = refined.clone();
                move |_: &crate::settings::RefineSettings| -> Box<dyn crate::refine::Refiner> {
                    Box::new(crate::refine::test_support::Stub::new(refined.clone(), |t| Ok(t.to_string())))
                }
            })));
            crate::settings::set_refine_settings_cache(crate::settings::RefineSettings {
                enabled: true,
                ..crate::settings::RefineSettings::default()
            });

            // An ask is answering in its panel.
            let worker = ask_in_the_background();
            // Meanwhile a dictation was recorded, with Refine warmed
            // for it, and is now being transcribed.
            assert_eq!(dictation_request_toggle(), TOGGLE_BEGIN_RECORDING);
            dictation_mark_capture_started();
            crate::refine::warm_for_recording();
            assert_eq!(dictation_request_toggle(), TOGGLE_STOP_RECORDING);
            dictation_mark_transcribing_pending();
            assert_eq!(refined.lock().unwrap().warmed, 1);

            // The cancel key: nothing is recording, an ask is busy.
            assert!(cancel_would_act());
            let env = InlineEnv::default();
            assert!(run_cancel(&env));

            // The ask is stopped.
            assert_eq!(worker.join().unwrap().unwrap().phase, AskPhase::Cancelled);
            // The dictation is where it was: still being transcribed,
            // its Refine backend still warm, no media resumed under
            // it.
            assert_eq!(dictation_snapshot().phase(), PHASE_TRANSCRIBING);
            assert_eq!(refined.lock().unwrap().dropped, 0, "the dictation's warmed Refine backend was dropped");
            assert_eq!(env.resumed.load(Ordering::SeqCst), 0);
            // And its text still goes through that backend.
            assert_eq!(refine_stage("one two three four five").trim(), "one two three four five");
            assert_eq!(refined.lock().unwrap().refined.len(), 1);

            // Once the ask is over too, the key does nothing at all.
            assert!(!run_cancel(&env));
            assert_eq!(dictation_snapshot().phase(), PHASE_TRANSCRIBING);

            crate::refine::test_support::set_factory(None);
        }

        #[test]
        fn a_late_close_of_one_ask_leaves_the_next_one_alone() {
            let _lock = start();
            let calls = Arc::new(Mutex::new(Calls::default()));
            let _scope = scope(on(), {
                let calls = calls.clone();
                move |_| Box::new(Stub::saying(calls.clone(), "Four."))
            });
            let env = InlineEnv::default();

            // An ask recording that turns out empty: the stop
            // pipeline has read which ask it is for, the phase is
            // DONE, and before the pipeline closes the ask...
            assert_eq!(dictation_request_toggle_kind(SessionKind::Ask), TOGGLE_BEGIN_RECORDING);
            dictation_mark_capture_started();
            ask_begin(&env);
            let first = with_state(|s| s.ask_id);
            assert_eq!(ask::snapshot().id, first);
            dictation_mark_transcribing_pending();
            dictation_mark_capture_stopped(0);
            assert_eq!(dictation_snapshot().phase(), PHASE_DONE);

            // ...the person starts the next ask.
            assert_eq!(dictation_request_toggle_kind(SessionKind::Ask), TOGGLE_BEGIN_RECORDING);
            dictation_mark_capture_started();
            ask_begin(&env);
            let second = ask::snapshot().id;
            assert_ne!(second, first);

            // The first pipeline's close, and the panel's close and
            // cancel of the ask it was showing, all land now.
            ask_dismiss(Some(first));
            run_dismiss_ask(&env, Some(first));
            assert!(!run_cancel_ask(&env, Some(first)));
            assert_eq!(ask::snapshot().id, second);
            assert_eq!(ask::snapshot().phase, AskPhase::Listening);
            assert!(is_recording(), "the new recording was cancelled");
            assert_eq!(calls.lock().unwrap().dropped, 1, "only the first ask's backend is gone");

            // Meant for the new one, they work.
            dictation_mark_transcribing_pending();
            dictation_mark_capture_stopped(16000);
            assert!(run_cancel_ask(&env, Some(second)));
            assert_eq!(ask::snapshot().phase, AskPhase::Idle);
        }

        #[test]
        fn a_recognizer_that_fails_while_the_question_is_recorded_ends_the_ask_with_the_reason() {
            let _lock = start();
            let calls = Arc::new(Mutex::new(Calls::default()));
            let _scope = scope(on(), {
                let calls = calls.clone();
                move |_| Box::new(Stub::saying(calls.clone(), "Four."))
            });
            assert_eq!(dictation_request_toggle_kind(SessionKind::Ask), TOGGLE_BEGIN_RECORDING);
            dictation_mark_capture_started();
            ask_begin(&InlineEnv::default());
            let id = ask::snapshot().id;
            assert_eq!(ask::snapshot().phase, AskPhase::Listening);

            // What the recognizer load thread does when it fails.
            dictation_deliver_error("recognizer load failed: model file missing");

            assert_eq!(dictation_snapshot().phase(), PHASE_ERROR);
            let asked = ask::snapshot();
            assert_eq!(asked.id, id);
            assert_eq!(asked.phase, AskPhase::Error, "the ask stayed on listening");
            assert_eq!(
                asked.error,
                "The question could not be transcribed (recognizer load failed: model file missing)."
            );
            assert!(!cancel_would_act());
            assert_eq!(calls.lock().unwrap().dropped, 1, "the warmed backend is gone");
            assert!(calls.lock().unwrap().sent.is_empty());

            // A later error of the dictation machine does not reach
            // an ask that is past listening.
            assert_eq!(ask::submit("what is two plus two").unwrap().phase, AskPhase::Done);
            with_state(|s| {
                s.kind = SessionKind::Ask;
                s.ask_id = ask::snapshot().id;
            });
            dictation_deliver_error("recognizer load: boom");
            assert_eq!(ask::snapshot().phase, AskPhase::Done);
        }

        #[test]
        fn closing_the_panel_clears_a_finished_answer_and_cancel_leaves_it() {
            let _lock = start();
            let calls = Arc::new(Mutex::new(Calls::default()));
            let _scope = scope(on(), {
                let calls = calls.clone();
                move |_| Box::new(Stub::saying(calls.clone(), "Four."))
            });
            assert_eq!(ask::submit("what is two plus two").unwrap().phase, AskPhase::Done);
            // Nothing to cancel: the answer stays for copy and insert.
            assert!(!run_cancel_ask(&InlineEnv::default(), None));
            assert!(!run_cancel(&InlineEnv::default()));
            assert_eq!(ask::finished_answer().as_deref(), Some("Four."));
            run_dismiss_ask(&InlineEnv::default(), None);
            assert_eq!(ask::finished_answer(), None);
            assert_eq!(ask::snapshot().phase, AskPhase::Idle);
        }

        #[test]
        fn an_empty_ask_recording_closes_the_ask_and_a_failed_one_ends_it_in_error() {
            let _lock = start();
            let calls = Arc::new(Mutex::new(Calls::default()));
            let _scope = scope(on(), {
                let calls = calls.clone();
                move |_| Box::new(Stub::saying(calls.clone(), "Four."))
            });
            // What the stop pipeline does when the microphone gave
            // nothing, and when the recognizer fails.
            for failure in ["empty", "error"] {
                assert_eq!(dictation_request_toggle_kind(SessionKind::Ask), TOGGLE_BEGIN_RECORDING);
                dictation_mark_capture_started();
                ask_begin(&InlineEnv::default());
                assert_eq!(ask::snapshot().phase, AskPhase::Listening);
                dictation_mark_transcribing_pending();
                let id = with_state(|s| s.ask_id);
                if failure == "empty" {
                    dictation_mark_capture_stopped(0);
                    ask_dismiss(Some(id));
                    assert_eq!(dictation_snapshot().phase(), PHASE_DONE);
                    assert_eq!(ask::snapshot().phase, AskPhase::Idle);
                } else {
                    dictation_mark_capture_stopped(16000);
                    dictation_deliver_error("transcribe: boom");
                    assert_eq!(dictation_snapshot().phase(), PHASE_ERROR);
                    assert_eq!(ask::snapshot().phase, AskPhase::Error);
                    assert_eq!(ask::snapshot().error, "The question could not be transcribed (transcribe: boom).");
                }
                assert!(!cancel_would_act(), "{failure}");
            }
            // Nothing heard at all: the ask ends idle, no call.
            record_ask(&InlineEnv::default());
            ask_stage("  ");
            assert_eq!(ask::snapshot().phase, AskPhase::Idle);
            assert!(calls.lock().unwrap().sent.is_empty());
        }

        #[test]
        fn with_ask_switched_off_an_ask_recording_goes_nowhere() {
            let _lock = start();
            let calls = Arc::new(Mutex::new(Calls::default()));
            let _scope = scope(AskSettings::default(), {
                let calls = calls.clone();
                move |_| Box::new(Stub::saying(calls.clone(), "Four."))
            });
            assert!(!ask_enabled());
            let injected_before = injected();
            // Even if a recording of that kind got started somehow.
            record_ask(&InlineEnv::default());
            ask_stage("what is two plus two");
            assert_eq!(ask::snapshot().phase, AskPhase::Idle);
            assert_eq!(calls.lock().unwrap().made, 0);
            assert_eq!(injected(), injected_before, "the question was typed as a dictation");
            assert_eq!(dictation_snapshot().transcript(), "");
            assert_eq!(dictation_snapshot().phase(), PHASE_DONE);
        }
    }
}
