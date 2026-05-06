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
    })
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
    })
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
    })
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
    })
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
