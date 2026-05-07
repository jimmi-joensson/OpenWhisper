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

use std::path::Path;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use crate::model_lifecycle::{ModelClaim, ModelHandle};

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
        ModelHandle::with_idle_timeout_and_claim(
            "recognizer",
            || {
                let mut backend = default_backend();
                backend.ensure_loaded()?;
                Ok(backend)
            },
            recognizer_claim(),
            RECOGNIZER_IDLE_TIMEOUT,
        )
    })
}

/// Static claim describing how much system memory the loaded
/// Parakeet recognizer occupies, and which OS pool holds it. Surfaced
/// via [`crate::telemetry::collect_memory_stats`] so the Diagnostics
/// readout can show ANE-resident weights on macOS — `last_load_rss_delta`
/// only sees the CoreML driver bookkeeping (~50 MB), not the ~460 MB
/// of weights that actually sit in the ANE pool.
///
/// On Mac the weight files live under FluidAudio's app-support cache;
/// we walk the model directory if it exists. On Windows the weights
/// are ONNX files in our own download cache; we sum their on-disk
/// sizes. In both cases the claim is the on-disk footprint, which
/// closely tracks the resident memory cost (mmap'd weights fault into
/// resident memory on first inference).
fn recognizer_claim() -> ModelClaim {
    let claimed_bytes = measure_recognizer_weight_bytes()
        .unwrap_or(PARAKEET_WEIGHT_BYTES_FALLBACK);
    ModelClaim {
        claimed_bytes,
        in_process: cfg!(not(target_os = "macos")),
    }
}

/// Conservative fallback for Parakeet-TDT 0.6B v3's on-disk weight
/// footprint when the platform-specific measurement fails (e.g. the
/// model hasn't been downloaded yet, or the cache path moved).
/// 460 MB rounded to a whole-MB constant so the readout stays stable.
const PARAKEET_WEIGHT_BYTES_FALLBACK: u64 = 460 * 1024 * 1024;

#[cfg(target_os = "macos")]
fn measure_recognizer_weight_bytes() -> Option<u64> {
    // FluidAudio caches FluidInference's `.mlmodelc` bundles under
    // `~/Library/Application Support/FluidAudio/Models/<id>`. The
    // bundle is a directory of binary blobs (Encoder/Decoder/etc.),
    // so we recursively sum file sizes. Falls back to the constant
    // if the cache is missing — common on first launch before the
    // bridge has been asked to load anything.
    let home = std::env::var_os("HOME")?;
    let base = Path::new(&home)
        .join("Library/Application Support/FluidAudio/Models");
    let candidates = [
        "parakeet-tdt-0.6b-v3",
        "parakeet-tdt-0.6b-v2",
    ];
    for name in candidates {
        let p = base.join(name);
        if p.is_dir() {
            return dir_bytes(&p);
        }
    }
    None
}

#[cfg(not(target_os = "macos"))]
fn measure_recognizer_weight_bytes() -> Option<u64> {
    // sherpa/ort path keeps its weights under our download cache. We
    // sum every file in the model dir (encoder/decoder/joiner ONNX +
    // tokens.txt). Non-blocking: returns None before the model has
    // been downloaded, so the readout falls back to the constant.
    dir_bytes(&download::cached_model_dir()?)
}

fn dir_bytes(dir: &Path) -> Option<u64> {
    let mut total = 0u64;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let entries = std::fs::read_dir(&d).ok()?;
        for entry in entries.flatten() {
            let meta = entry.metadata().ok()?;
            if meta.is_dir() {
                stack.push(entry.path());
            } else if meta.is_file() {
                total = total.saturating_add(meta.len());
            }
        }
    }
    if total == 0 { None } else { Some(total) }
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
