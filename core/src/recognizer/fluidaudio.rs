//! FluidAudio bridge backend (macOS only).
//!
//! Calls into a small Swift staticlib at `core/swift/FluidAudioBridge`
//! that wraps FluidAudio's `AsrManager`. FluidAudio loads FluidInference's
//! pre-converted Parakeet v3 .mlmodelc artifact and runs it on the ANE —
//! the path the shipped Mac SwiftUI app uses today.
//!
//! Why a Swift bridge instead of driving CoreML from Rust:
//! `project_stt_engine` memory rules out hand-rolling NeMo→CoreML
//! conversion. FluidInference owns that conversion + the ANE-tuned
//! `.mlmodelc`, but their API is Swift-only. See the bench decision in
//! `backlog/decisions/decision-1 - Recognizer bench thresholds.md` for why
//! sherpa-onnx + ONNX→CoreML EP didn't engage the ANE.

use std::ffi::{CStr, c_char};

use super::{Recognizer, TranscribeResult};

unsafe extern "C" {
    fn fab_load() -> i32;
    fn fab_transcribe(
        samples: *const f32,
        count: u64,
        out_confidence: *mut f32,
    ) -> *mut c_char;
    fn fab_free_string(ptr: *mut c_char);
    fn fab_last_error() -> *const c_char;
}

pub struct FluidAudioBridge;

impl FluidAudioBridge {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FluidAudioBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl Recognizer for FluidAudioBridge {
    fn ensure_loaded(&mut self) -> Result<(), String> {
        let rc = unsafe { fab_load() };
        if rc == 0 {
            Ok(())
        } else {
            Err(last_error().unwrap_or_else(|| format!("fab_load returned {rc}")))
        }
    }

    fn transcribe(&mut self, samples: &[f32]) -> Result<TranscribeResult, String> {
        let mut conf: f32 = 0.0;
        let raw = unsafe {
            fab_transcribe(
                samples.as_ptr(),
                samples.len() as u64,
                &mut conf as *mut f32,
            )
        };
        if raw.is_null() {
            return Err(last_error().unwrap_or_else(|| "fab_transcribe failed".to_string()));
        }
        let text = unsafe { CStr::from_ptr(raw) }
            .to_string_lossy()
            .into_owned();
        unsafe { fab_free_string(raw) };
        Ok(TranscribeResult {
            text,
            confidence: conf,
            elapsed_ms: 0,
        })
    }

    fn active_ep(&self) -> Option<String> {
        // FluidInference's Parakeet artifact is ANE-tuned; the
        // .mlmodelc consistently dispatches to the Apple Neural
        // Engine on M-series. No probing API to confirm at runtime
        // — static label.
        Some("ANE".to_string())
    }
}

fn last_error() -> Option<String> {
    let ptr = unsafe { fab_last_error() };
    if ptr.is_null() {
        return None;
    }
    Some(unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned())
}
