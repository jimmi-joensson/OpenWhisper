//! In-core speech recognition.
//!
//! Trait + global engine that the Tauri shell drives. The shell drains
//! `audio_drain_samples()` on stop and hands the buffer to
//! `recognizer_transcribe`. Result flows back into the dictation state
//! machine via `dictation::dictation_deliver_transcript`.
//!
//! Per the bench decisions in
//! `backlog/decisions/recognizer-bench-thresholds-2026-04-26.md` and
//! `backlog/decisions/recognizer-ort-engine-2026-04-26.md`, sherpa-onnx
//! + ONNX→CoreML EP did not engage the ANE on macOS — the Mac path uses
//! FluidAudio (FluidInference's ANE-tuned `.mlmodelc`) through a Swift
//! `@_cdecl` bridge. Windows runs Parakeet-TDT-v3 directly through the
//! `ort` Rust crate on CPU EP (DML / CUDA / TRT compiled-in via opt-in
//! Cargo features). Both impls hide behind the `Recognizer` trait so the
//! call site is OS-agnostic.
//!
//! Parakeet-TDT v3 is offline (batch). There are no real partials; the
//! shell pumps the full waveform after stop and gets a single result.

use std::sync::{Mutex, OnceLock};
use std::time::Instant;

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
}

static ENGINE: OnceLock<Mutex<Box<dyn Recognizer>>> = OnceLock::new();

#[cfg(target_os = "macos")]
fn default_backend() -> Box<dyn Recognizer> {
    Box::new(FluidAudioBridge::new())
}

#[cfg(not(target_os = "macos"))]
fn default_backend() -> Box<dyn Recognizer> {
    Box::new(OrtParakeet::new())
}

/// Wire up the platform default backend. Idempotent. The shell calls this
/// lazily — failing here surfaces via `dictation_deliver_error` upstream.
pub fn recognizer_ensure_loaded() -> Result<(), String> {
    let mutex = ENGINE.get_or_init(|| Mutex::new(default_backend()));
    let mut guard = mutex.lock().map_err(|_| "recognizer mutex poisoned".to_string())?;
    guard.ensure_loaded()
}

/// Decode samples on the calling thread. Caller is expected to be a
/// worker — Tauri shell does this off the UI thread.
pub fn recognizer_transcribe(samples: &[f32]) -> Result<TranscribeResult, String> {
    let mutex = ENGINE
        .get()
        .ok_or_else(|| "recognizer not initialized".to_string())?;
    let mut guard = mutex.lock().map_err(|_| "recognizer mutex poisoned".to_string())?;
    let t0 = Instant::now();
    let mut result = guard.transcribe(samples)?;
    // Backends are free to set their own elapsed_ms; default to wall time
    // for impls that leave it at 0.
    if result.elapsed_ms == 0 {
        result.elapsed_ms = t0.elapsed().as_millis() as u64;
    }
    Ok(result)
}
