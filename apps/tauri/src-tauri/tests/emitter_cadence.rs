//! Regression guard for TASK-79: the dictation emitter loop must never
//! call IO that can block longer than a single tick budget.
//!
//! The original bug was a 2s-cadence stutter on the level stream because
//! `compute_audio_device_state()` (cpal enumerate, per-device CoreAudio I/O)
//! ran inline in the emitter loop. Moving it to a dedicated poller fixed
//! the symptom; this test fixes the regression.
//!
//! AC #6 originally specified Playwright, but the existing Playwright
//! suite drives a stubbed Tauri runtime — it can't observe real emitter
//! cadence. This Rust integration test substitutes by timing the per-
//! iteration work the emitter performs (snapshot + level read) and
//! asserting p99 stays well below the 50 ms tick budget. If a future
//! change re-introduces blocking IO into either function, this test goes
//! red.

use std::time::{Duration, Instant};

use openwhisper_core::{audio, dictation};

const ITERATIONS: usize = 400;
// Emitter target per-iteration budget per TASK-79 AC #2 (~10 ms). We use
// a generous 25 ms ceiling here so warm CI / loaded dev hosts don't flake
// — anything resembling the original blocking-cpal regression (50–500 ms)
// will blow through this by an order of magnitude.
const P99_BUDGET_MS: u128 = 25;

#[test]
fn emitter_per_iteration_work_is_non_blocking() {
    let mut samples: Vec<u128> = Vec::with_capacity(ITERATIONS);
    for _ in 0..ITERATIONS {
        let t = Instant::now();
        let _snap = dictation::dictation_snapshot();
        let _level = audio::audio_current_level();
        samples.push(t.elapsed().as_micros());
        // Tiny yield so the loop doesn't pin a core; doesn't affect the
        // measured per-iteration timings.
        std::thread::sleep(Duration::from_micros(100));
    }

    samples.sort_unstable();
    let p50 = samples[samples.len() / 2];
    let p99 = samples[(samples.len() * 99) / 100];
    let max = *samples.last().unwrap();

    let p50_ms = p50 as f64 / 1000.0;
    let p99_ms = p99 as f64 / 1000.0;
    let max_ms = max as f64 / 1000.0;

    eprintln!(
        "emitter per-iter work: p50={p50_ms:.3} ms p99={p99_ms:.3} ms max={max_ms:.3} ms"
    );

    assert!(
        (p99 / 1000) <= P99_BUDGET_MS,
        "emitter per-iteration work p99={p99_ms:.3} ms exceeded {P99_BUDGET_MS} ms budget — \
         a blocking call may have been re-introduced into the dictation_tick path"
    );
}
