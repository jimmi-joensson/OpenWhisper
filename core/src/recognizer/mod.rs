//! In-core speech recognition.
//!
//! Trait + global engine that the Tauri shell drives. The shell drains
//! `audio_drain_samples()` on stop and hands the buffer to
//! `recognizer_transcribe`. Result flows back into the dictation state
//! machine via `dictation::dictation_deliver_transcript`.
//!
//! Per the bench decisions in
//! `backlog/decisions/decision-1 - Recognizer bench thresholds.md` and
//! `backlog/decisions/decision-3 - Recognizer engine swap to ort.md`, sherpa-onnx
//! + ONNX→CoreML EP did not engage the ANE on macOS — the Mac path uses
//! FluidAudio (FluidInference's ANE-tuned `.mlmodelc`) through a Swift
//! `@_cdecl` bridge. Windows runs Parakeet-TDT-v3 directly through the
//! `ort` Rust crate on CPU EP (DML / CUDA / TRT compiled-in via opt-in
//! Cargo features). Both impls hide behind the `Recognizer` trait so the
//! call site is OS-agnostic.
//!
//! Parakeet-TDT v3 is offline (batch). There are no real partials; the
//! shell pumps the full waveform after stop and gets a single result.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use crate::model_lifecycle::ModelHandle;

#[cfg(target_os = "macos")]
mod fluidaudio;
#[cfg(not(target_os = "macos"))]
mod download;
#[cfg(not(target_os = "macos"))]
mod ep_probe;
#[cfg(not(target_os = "macos"))]
mod mel;
#[cfg(not(target_os = "macos"))]
mod ort_lib;
#[cfg(not(target_os = "macos"))]
mod ort_parakeet;

#[cfg(target_os = "macos")]
pub use fluidaudio::FluidAudioBridge;
#[cfg(not(target_os = "macos"))]
pub use ort_parakeet::OrtParakeet;

/// Outcome of a single utterance.
#[derive(Debug, Clone)]
pub struct TranscribeResult {
    pub text: String,
    /// FluidAudio reports a real confidence scalar; the ort Parakeet
    /// path doesn't surface one — Windows returns 1.0 placeholder.
    pub confidence: f32,
    pub elapsed_ms: u64,
}

/// Pluggable recognizer backend. Implementations own their model handle
/// and any background state. `transcribe` is `&mut self` so impls can
/// reuse a session/decoder buffer across calls.
pub trait Recognizer: Send {
    /// Idempotent: download model + load session on first call, no-op
    /// thereafter. Blocks until ready.
    fn ensure_loaded(&mut self) -> Result<(), String>;

    /// Decode a 16 kHz mono f32 buffer.
    fn transcribe(&mut self, samples: &[f32]) -> Result<TranscribeResult, String>;

    /// Active execution-provider label for diagnostics. Default
    /// returns `None`; concrete impls override (Mac FluidAudio → ANE;
    /// Windows ort → CPU/DirectML/CUDA depending on probe). Returns
    /// `None` until the engine has loaded — `ensure_loaded` first.
    fn active_ep(&self) -> Option<String> {
        None
    }
}

/// Engine wrapped in a `ModelHandle` so the model can auto-unload
/// after a configurable idle window (TASK-62). The platform default
/// backend is constructed AND `ensure_loaded`'d inside the loader
/// closure — `ModelHandle::load` returns only after the recognizer is
/// actually ready. Subsequent `use_with` calls reuse the resident
/// backend; the idle timer fires on a background thread to release
/// it after `RECOGNIZER_IDLE_TIMEOUT` of inactivity.
///
/// Manual smoke note: after wrapping, the first transcription after
/// `RECOGNIZER_IDLE_TIMEOUT` of idle re-enters `PHASE_LOADING_MODEL`
/// for ~200–500 ms (Mac CoreML compile-cache hit) then proceeds. On
/// Windows the cold path is ~100–300 ms (ONNX session creation).
static ENGINE: OnceLock<ModelHandle<Box<dyn Recognizer>>> = OnceLock::new();

/// Idle window before the recognizer auto-unloads. 5 min matches the
/// model-lifecycle spec
/// (`backlog/docs/specs/2026-05-01-model-lifecycle-telemetry.md`):
/// long enough that a normal "thinking → dictating again" pause keeps
/// the model warm, short enough that an unattended app reclaims the
/// hundreds of MB the model holds.
const RECOGNIZER_IDLE_TIMEOUT: Duration = Duration::from_secs(300);

fn engine() -> &'static ModelHandle<Box<dyn Recognizer>> {
    ENGINE.get_or_init(|| {
        ModelHandle::with_idle_timeout(
            "recognizer",
            || {
                let mut backend = default_backend();
                backend.ensure_loaded()?;
                Ok(backend)
            },
            RECOGNIZER_IDLE_TIMEOUT,
        )
    })
}

#[cfg(target_os = "macos")]
fn default_backend() -> Box<dyn Recognizer> {
    Box::new(FluidAudioBridge::new())
}

#[cfg(not(target_os = "macos"))]
fn default_backend() -> Box<dyn Recognizer> {
    Box::new(OrtParakeet::new())
}

/// Wire up the platform default backend. Idempotent. The shell calls
/// this lazily — failing here surfaces via `dictation_deliver_error`
/// upstream.
pub fn recognizer_ensure_loaded() -> Result<(), String> {
    engine().load()
}

/// Active execution-provider label for the loaded backend, or `None`
/// if the engine hasn't been initialized OR has since been
/// auto-released by the idle timer. Used by `core::diagnostics` to
/// surface the engaged EP in `RecognizerInfo`. Does NOT trigger an
/// auto-load — diagnostics readouts must be cheap.
pub fn active_ep() -> Option<String> {
    let h = ENGINE.get()?;
    h.try_inspect(|r| r.active_ep()).flatten()
}

/// Decode samples on the calling thread. Caller is expected to be a
/// worker — Tauri shell does this off the UI thread. Auto-loads the
/// recognizer if it was previously unloaded (idle-released or never
/// loaded); the cold-load latency surfaces upstream via
/// `PHASE_LOADING_MODEL` if the caller drives the dictation flow.
pub fn recognizer_transcribe(samples: &[f32]) -> Result<TranscribeResult, String> {
    let t0 = Instant::now();
    // `use_with` returns Result<closure-result, String>; the closure
    // body itself returns Result<TranscribeResult, String>. Two
    // levels of `?` flatten both.
    let mut result = engine().use_with(|r| r.transcribe(samples))??;
    // Backends are free to set their own elapsed_ms; default to wall
    // time for impls that leave it at 0.
    if result.elapsed_ms == 0 {
        result.elapsed_ms = t0.elapsed().as_millis() as u64;
    }
    Ok(result)
}

/// Test/inspection hook: current lifecycle state of the recognizer
/// engine. Returns `None` if the engine hasn't been touched yet.
pub fn engine_state() -> Option<crate::model_lifecycle::LifecycleState> {
    ENGINE.get().map(|h| h.state())
}
