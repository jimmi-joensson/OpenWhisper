//! End-to-end smoke for the platform recognizer.
//!
//! Usage:
//!   cargo run --release --no-default-features --features recognizer \
//!     --example recognizer_smoke -- <16kHz-mono.wav>
//!
//! On macOS this drives FluidAudio (FluidInference Parakeet v3 .mlmodelc
//! on ANE). On Windows / Linux this drives sherpa-onnx + Parakeet v3
//! ONNX on CPU. Same Rust trait either way — see
//! `core/src/recognizer/mod.rs`.

use std::env;
use std::time::Instant;

use openwhisper_core::recognizer::{recognizer_ensure_loaded, recognizer_transcribe};

fn main() {
    let wav_path = env::args()
        .nth(1)
        .expect("usage: recognizer_smoke <wav-path>");

    eprintln!("[smoke] loading recognizer (downloads model on first run)…");
    let t_load = Instant::now();
    if let Err(e) = recognizer_ensure_loaded() {
        eprintln!("[smoke] ensure_loaded failed: {e}");
        std::process::exit(1);
    }
    eprintln!(
        "[smoke] recognizer ready in {} ms",
        t_load.elapsed().as_millis()
    );

    let samples = read_wav_mono_f32(&wav_path).expect("read wav");
    eprintln!(
        "[smoke] wav: samples={} ({:.2} s @ 16 kHz)",
        samples.len(),
        samples.len() as f32 / 16_000.0
    );

    // Two decodes — first = cold (model session warm-up may happen on
    // first inference depending on backend), second = steady-state.
    for i in 1..=2 {
        let t = Instant::now();
        let res = recognizer_transcribe(&samples).expect("transcribe");
        eprintln!(
            "[smoke] decode #{i} wall={} ms confidence={:.3}",
            t.elapsed().as_millis(),
            res.confidence
        );
        if i == 2 {
            println!("{}", res.text);
        }
    }
}

fn read_wav_mono_f32(path: &str) -> Result<Vec<f32>, String> {
    let mut reader = hound::WavReader::open(path).map_err(|e| format!("open wav: {e}"))?;
    let spec = reader.spec();
    if spec.sample_rate != 16_000 {
        eprintln!("[smoke] WARN: wav is {} Hz; expected 16000", spec.sample_rate);
    }
    if spec.channels != 1 {
        return Err(format!(
            "wav has {} channels; expected mono",
            spec.channels
        ));
    }
    match spec.sample_format {
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            Ok(reader
                .samples::<i32>()
                .map(|s| s.unwrap() as f32 / max)
                .collect())
        }
        hound::SampleFormat::Float => Ok(reader
            .samples::<f32>()
            .map(|s| s.unwrap())
            .collect()),
    }
}
