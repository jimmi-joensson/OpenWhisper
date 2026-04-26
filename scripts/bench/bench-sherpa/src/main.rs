//! Sherpa-onnx + Parakeet v3 + CoreML EP arm of the TASK-33 bench.
//!
//! Output: single JSON line on stdout so `run.sh` can parse and append to
//! the decision doc. Stderr carries human-readable progress + the model
//! download log (first run only).

use std::env;
use std::time::Instant;

use openwhisper_core::recognizer::{recognizer_ensure_loaded, recognizer_transcribe};
use sherpa_onnx::Wave;

fn main() {
    let wav_path = env::args()
        .nth(1)
        .expect("usage: bench-sherpa <wav-path>");

    let t_load = Instant::now();
    if let Err(e) = recognizer_ensure_loaded() {
        eprintln!("[bench-sherpa] load failed: {e}");
        std::process::exit(1);
    }
    let load_ms = t_load.elapsed().as_millis();

    let wave = Wave::read(&wav_path).expect("read wave");
    if wave.sample_rate() != 16_000 {
        eprintln!(
            "[bench-sherpa] WARN: wav is {} Hz, Parakeet expects 16000",
            wave.sample_rate()
        );
    }
    let dur_s = wave.samples().len() as f32 / wave.sample_rate() as f32;

    let t_dec = Instant::now();
    let res = recognizer_transcribe(wave.samples()).expect("transcribe");
    let dec_ms = t_dec.elapsed().as_millis();

    // Single-line JSON: keep it small + grep-friendly. Quote the text so
    // newlines/special chars survive jq.
    println!(
        "{{\"engine\":\"sherpa-onnx\",\"clip\":{:?},\"clip_seconds\":{:.3},\"load_ms\":{},\"decode_ms\":{},\"text\":{:?}}}",
        wav_path, dur_s, load_ms, dec_ms, res.text
    );
}
