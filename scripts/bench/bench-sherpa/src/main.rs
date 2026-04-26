//! Cross-platform recognizer bench runner. One-shot: load model, decode
//! the supplied wav, print a single JSON line on stdout (consumed by
//! `smoke-with-powermetrics.sh` on Mac and `smoke-with-wpr.ps1` on
//! Windows). Stderr carries human-readable progress and the first-run
//! model download log.

use std::env;
use std::time::Instant;

use openwhisper_core::recognizer::{recognizer_ensure_loaded, recognizer_transcribe};

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

    let samples = read_wav_mono_f32(&wav_path).expect("read wav");
    let dur_s = samples.len() as f32 / 16_000.0;

    let t_dec = Instant::now();
    let res = recognizer_transcribe(&samples).expect("transcribe");
    let dec_ms = t_dec.elapsed().as_millis();

    // Single-line JSON: keep it small + grep-friendly. Quote the text so
    // newlines/special chars survive jq. `provider` reflects the EP that
    // actually engaged (set via `OPENWHISPER_PROVIDER`); the binary name
    // stays `bench-sherpa` for now to avoid churning the harness scripts.
    let provider = std::env::var("OPENWHISPER_PROVIDER").unwrap_or_else(|_| "cpu".to_string());
    println!(
        "{{\"engine\":\"ort\",\"provider\":{provider:?},\"clip\":{wav_path:?},\"clip_seconds\":{dur_s:.3},\"load_ms\":{load_ms},\"decode_ms\":{dec_ms},\"text\":{:?}}}",
        res.text
    );
}

fn read_wav_mono_f32(path: &str) -> Result<Vec<f32>, String> {
    let mut reader = hound::WavReader::open(path).map_err(|e| format!("open wav: {e}"))?;
    let spec = reader.spec();
    if spec.sample_rate != 16_000 {
        eprintln!(
            "[bench-sherpa] WARN: wav is {} Hz, Parakeet expects 16000",
            spec.sample_rate
        );
    }
    if spec.channels != 1 {
        return Err(format!("wav has {} channels; expected mono", spec.channels));
    }
    match spec.sample_format {
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            Ok(reader
                .samples::<i32>()
                .map(|s| s.unwrap() as f32 / max)
                .collect())
        }
        hound::SampleFormat::Float => Ok(reader.samples::<f32>().map(|s| s.unwrap()).collect()),
    }
}
