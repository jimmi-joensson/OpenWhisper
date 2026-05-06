//! End-to-end smoke for `openwhisper transcribe`. Spawns the bin
//! against `tests/fixtures/hello-world.wav` (16 kHz mono i16 PCM)
//! and asserts the recognizer produced a non-empty transcript that
//! contains "hello" (case-insensitive). Exercises the full pipeline
//! — WAV decode, recognizer load, transcribe, transcript filter.
//!
//! On Mac this runs the FluidAudio + ANE path; on Win/Linux it
//! runs ort + sherpa-onnx Parakeet on whichever EP the probe
//! picks (CPU on a clean CI runner).

use std::path::PathBuf;
use std::process::Command;

#[test]
fn transcribe_emits_non_empty_text_containing_hello() {
    let exe = env!("CARGO_BIN_EXE_openwhisper");
    let mut wav = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    wav.push("tests/fixtures/hello-world.wav");
    assert!(wav.is_file(), "fixture missing: {}", wav.display());

    let out = Command::new(exe)
        .arg("--json")
        .arg("transcribe")
        .arg(&wav)
        .output()
        .expect("spawn openwhisper");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "cli failed (status={:?})\nstderr:\n{stderr}",
        out.status,
    );

    let value: serde_json::Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("stdout not valid JSON: {e}\nstdout:\n{}", String::from_utf8_lossy(&out.stdout)));
    let text = value["text"].as_str().expect("missing text field");
    assert!(!text.is_empty(), "transcript was empty");
    assert!(
        text.to_lowercase().contains("hello"),
        "expected 'hello' in transcript, got: {text:?}",
    );

    let confidence = value["confidence"].as_f64().expect("missing confidence");
    assert!(
        (0.0..=1.0).contains(&confidence),
        "confidence out of range: {confidence}",
    );

    // duration_ms is the wall-clock decode time. Sanity bound:
    // > 0 (we did real work) and < 60_000 (fixture is ~5s and ANE
    // does it in <200 ms; CPU-EP CI runs in low seconds).
    let duration = value["duration_ms"].as_u64().expect("missing duration_ms");
    assert!(duration > 0, "duration_ms was zero");
    assert!(duration < 60_000, "duration_ms suspiciously high: {duration}");
}
