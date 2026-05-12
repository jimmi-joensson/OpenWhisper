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

struct State {
    phase: u32,
    status_message: String,
    transcript: String,
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

impl State {
    fn new() -> Self {
        Self {
            phase: PHASE_IDLE,
            status_message: "idle — tap Record, speak, tap again".to_string(),
            transcript: String::new(),
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
pub struct DictationSnapshot {
    phase: u32,
    status_message: String,
    transcript: String,
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
    pub fn status_message(&self) -> String {
        self.status_message.clone()
    }
    pub fn transcript(&self) -> String {
        self.transcript.clone()
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
        status_message: s.status_message.clone(),
        transcript: s.transcript.clone(),
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
    with_state(|s| {
        if !s.can_toggle() {
            return TOGGLE_IGNORE;
        }
        if s.phase == PHASE_RECORDING {
            TOGGLE_STOP_RECORDING
        } else {
            // Fresh session — clear previous transcript/error so the UI
            // doesn't briefly show stale content while the shell spins up.
            s.transcript.clear();
            s.confidence = 0.0;
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
        s.transcript.clear();
        s.confidence = 0.0;
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

pub fn dictation_deliver_error(message: &str) {
    IS_RECORDING.store(false, Ordering::Relaxed);
    with_state(|s| {
        s.error_message = message.to_string();
        s.status_message = message.to_string();
        s.phase = PHASE_ERROR;
        s.record_start = None;
        s.record_start_epoch_ms = None;
        s.voiced_ms_at_drain = None;
        s.download_bytes_done = 0;
        s.download_bytes_total = 0;
    });
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
    let action = dictation_request_toggle();
    match action {
        TOGGLE_BEGIN_RECORDING => {
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
            crate::audio::audio_start_capture()?;
            dictation_mark_capture_started();
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

/// Run a user-initiated cancel. Stops audio capture, drains the buffer
/// (samples are discarded), flips the phase machine to IDLE without
/// emitting a transcript, and resumes paused media. Returns `true` if a
/// recording was actually cancelled.
#[cfg(feature = "recognizer")]
pub fn run_cancel<E: DictationEnv>(env: &E) -> bool {
    crate::audio::audio_stop_capture();
    let _ = crate::audio::audio_drain_samples();
    let cancelled = dictation_request_cancel();
    env.resume_audio();
    cancelled
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
        return;
    }
    // Defensive: recognizer_transcribe requires the engine to be
    // initialized. Loader was kicked off at recording start, but a slow
    // first-load might still be in flight — re-call ensure_loaded so we
    // block until it's ready.
    let t_load = std::time::Instant::now();
    if let Err(e) = crate::recognizer::recognizer_ensure_loaded() {
        dictation_deliver_error(&format!("recognizer load: {e}"));
        return;
    }
    let load_ms = t_load.elapsed().as_millis();
    let t_tx = std::time::Instant::now();
    match crate::recognizer::recognizer_transcribe(&samples) {
        Ok(res) => {
            let transcribe_ms = t_tx.elapsed().as_millis();
            let cleaned = crate::transcript::process(&res.text);
            crate::verbose_log!(
                "[ow.dictation] stop drain_ms={drain_ms} ensure_loaded_ms={load_ms} \
                 transcribe_ms={transcribe_ms} samples={count} chars={} confidence={:.2}",
                cleaned.len(),
                res.confidence
            );
            dictation_deliver_transcript(&cleaned, res.confidence);
        }
        Err(e) => dictation_deliver_error(&format!("transcribe: {e}")),
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

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, MutexGuard};

    use super::*;

    // Tests mutate the global STATE singleton. Serialize them with a
    // dedicated test-only mutex so parallel execution doesn't corrupt
    // state between assertions.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

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
}
