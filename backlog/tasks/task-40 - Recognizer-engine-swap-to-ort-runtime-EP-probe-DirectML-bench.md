---
id: TASK-40
title: Recognizer engine swap to `ort` + runtime EP probe + DirectML bench
status: To Do
assignee:
  - claude
created_date: '2026-04-26 20:15'
labels:
  - recognizer
  - windows
  - gpu
  - directml
  - architecture
dependencies:
  - TASK-39
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-39 proved CUDA EP via sherpa-onnx is a dead end on Parakeet-TDT-v3-int8 (RTX 3070 SM peaked at 4–8% during decode; CUDA decode 41% slower than CPU best of 656 ms). The architectural mismatch — small int8 transducer with sequential joint network — applies to *any* generic ONNX EP, not just CUDA. See `backlog/decisions/recognizer-cuda-decision-2026-04-26.md`.

But the *engine plumbing* is still wrong for what we want to ship. Today's recognizer is hardcoded to "sherpa-onnx → CPU EP only on Windows." User requirement (2026-04-26): the Windows app should pick the optimal inference path **automatically** based on what's on the box — CPU only, NVIDIA GPU, AMD GPU, Intel iGPU — without per-vendor builds. The current sherpa-onnx wiring can't do that without each-vendor-source-build gymnastics.

The fix has three parts:

1. **Swap `sherpa-onnx` for the `ort` Rust crate** (https://github.com/pykeio/ort). It's the de-facto Rust binding to ONNXRuntime, supports DirectML / CUDA / TensorRT / CoreML EPs cleanly through one API, and consumes the same Parakeet `.onnx` files the current shell already downloads. Cost: ~200 LOC of Rust to write the RNN-T greedy decoder that sherpa-onnx wraps for us today (encoder/decoder/joiner ONNX sessions chained in a token-emit loop).

2. **Runtime EP probe at startup**, in priority order: TensorRT (NVIDIA + TRT runtime present) → CUDA (NVIDIA + cuDNN present) → DirectML (any DirectX 12 GPU, including integrated) → CPU. Try each in order; first that builds an `Session` successfully wins. Log which one engaged so users + future bench runs can verify. Each probe needs a fail-fast timeout so a misconfigured GPU driver doesn't stall startup.

3. **Bundle DirectML by default** (~30–50 MB additional DLLs vs CUDA's 1.7 GB floor). DirectML EP works on every Windows 10 1903+ box with a DX12 GPU — that's effectively all hardware OpenWhisper would target. CUDA stays available for NVIDIA users who explicitly want it (separate optional download) but isn't in the default bundle. CPU-only is always the floor.

Bench all three Windows paths (CPU, DirectML, CUDA-if-installed) end-to-end before flipping the runtime probe on. Set per-host thresholds: an EP that's measurably slower than CPU on a given box auto-disables itself for that host (cached after first launch). Don't ship the probe unless DirectML clears CPU on at least one of (RTX 3070 / AMD discrete / Intel Arc / Intel iGPU) hardware classes — otherwise the EP probe is just complexity for no win on Parakeet-TDT-v3, and we keep the simpler CPU-only default.

Multilingual: no model change needed. The shipped `sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8` is already the multilingual variant (25 European languages including Danish — `project_parakeet_v3_multilingual_behavior` memory). This task only changes the inference engine, not the weights.

**Non-goal**: Mac path. `target_os = "macos"` continues to use FluidAudio via the Swift bridge — `Recognizer` trait abstraction is unchanged. This is purely a Windows-side engine swap.

**Non-goal**: Whisper / model swap. A faster-whisper / whisper.cpp evaluation is a separate, larger task gated on a fresh quality bench (different model = different WER/hallucination profile). Stay on Parakeet-TDT-v3 multilingual.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria

<!-- AC:BEGIN -->
- [ ] #1 `core/Cargo.toml` Windows recognizer feature swaps `sherpa-onnx` dep for `ort` crate (latest 2.x); sherpa-onnx-sys removed from the build closure on Windows
- [ ] #2 `core/src/recognizer/sherpa.rs` renamed/replaced by `ort_parakeet.rs` (or similar); implements the same `Recognizer` trait. Includes a Rust RNN-T greedy decoder over the encoder/decoder/joiner ONNX sessions
- [ ] #3 Recognizer download module unchanged — same Parakeet v3 int8 archive, same cache path; only the consumer changes
- [ ] #4 Runtime EP probe implemented: priority order TensorRT → CUDA → DirectML → CPU; logs which EP engaged at first session creation; each probe has a fail-fast guard so a broken EP doesn't stall startup
- [ ] #5 EP-selection cache: once a host has chosen an EP (or auto-disabled a slow one), the choice is remembered (e.g. `~/.cache/openwhisper/ep-pref.json`) so the probe doesn't re-run every boot. Override via `OPENWHISPER_PROVIDER` env var (matches existing TASK-39 knob)
- [ ] #6 Bench arm: extend `scripts/bench/bench-sherpa/` (rename to `bench-recognizer/` if you like) to run the same 5-rep harness against each available EP on the host. Append results to `scripts/bench/results/<host>-<date>.txt` mirroring TASK-39 format. Capture vendor-appropriate GPU util sample during decode (nvidia-smi for NVIDIA, GPU-Z / DXGI counters for DirectML, etc.)
- [ ] #7 Decision recorded in `backlog/decisions/recognizer-ort-engine-<date>.md`: which EPs cleared CPU on which hardware classes, whether the runtime probe ships in the Tauri bundle or stays gated, and the bundling story (DirectML default? CUDA optional download? CPU-only floor?)
- [ ] #8 If shipping the probe: Tauri bundler config updated to include the DirectML EP DLLs and the chosen onnxruntime build; bundle size delta measured and recorded in the decision doc
- [ ] #9 Mac path unchanged — `target_os = "macos"` arm still uses `FluidAudioBridge`. Verified by `cargo check --target aarch64-apple-darwin` (or by building the shipped Mac SwiftUI app, whichever is faster)
- [ ] #10 `OPENWHISPER_NUM_THREADS` env var continues to apply to the CPU EP path (TASK-39 default of `min(num_cpus::get_physical(), 8)` preserved)
<!-- AC:END -->

## Handover prompt

Paste the block below into a fresh Claude Code session on a Windows box (ideally the RTX 3070 from TASK-39 so we have a baseline; but also useful to run on an AMD or Intel-iGPU box to validate the cross-vendor claim).

```
# OpenWhisper recognizer engine swap (TASK-40, follow-up to TASK-39)

## Goal

Swap the Windows recognizer's inference engine from sherpa-onnx to the
`ort` Rust crate. Add a runtime EP probe (TensorRT → CUDA → DirectML →
CPU) so a single binary picks the optimal path per host without
per-vendor builds. Validate that DirectML actually wins (or doesn't) on
Parakeet-TDT-v3-int8 across at least one NVIDIA + one non-NVIDIA box
before flipping the probe on. See `backlog/tasks/task-40 - *.md` for the
full spec.

## Context — what's already there

- **TASK-39 just landed**:
  - `core/src/recognizer/sherpa.rs` — sherpa-onnx Recognizer impl. Reads
    `OPENWHISPER_NUM_THREADS` (defaults to `min(num_cpus::get_physical(), 8)`)
    and `OPENWHISPER_PROVIDER` from env.
  - CUDA EP empirically dead on Parakeet-TDT-v3-int8 (RTX 3070 SM peak 4%,
    decode 41% slower than CPU best of 656 ms). See
    `backlog/decisions/recognizer-cuda-decision-2026-04-26.md`.
  - The `recognizer` Cargo feature in `core/Cargo.toml` pulls
    `sherpa-onnx 1.12.40` with the `shared` feature. `num_cpus 1.16` was
    added for the thread-default heuristic.
- **Mac path**: `core/src/recognizer/fluidaudio.rs` via Swift FFI bridge.
  Untouched by this task. `target_os` cfg-gating keeps the two impls
  separate — see `core/src/recognizer/mod.rs`.
- **Bench harness**: `scripts/bench/bench-sherpa/` is a one-shot Rust bin
  that loads the recognizer, decodes a wav, prints JSON. Wrap or fork it.
- **Smoke clip**: `apps/macos/Resources/samples/smoke-test.wav` (5.06 s
  EN, baked into the repo). Use this for like-for-like comparison with
  TASK-39 numbers.
- **Model**: `sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8`, ~487 MB
  archive cached at `~/.cache/openwhisper/models/`. Four ONNX files:
  encoder.int8.onnx, decoder.int8.onnx, joiner.int8.onnx, tokens.txt.
  These are NeMo Parakeet TDT (token+duration) files — sherpa-onnx
  wraps the greedy decode loop today.

## What you need to do

### Phase 1 — Swap engine, keep CPU EP only

Don't try to do everything at once. First: replace sherpa-onnx with `ort`,
keep the CPU EP only, prove the same JSON output + same 656 ms decode
on the RTX 3070 baseline. If you can't match CPU perf to within 10%
with `ort`, debug before adding GPU EPs.

1. `cargo add ort@2 --features "load-dynamic"` in core (or pin a specific
   2.x version that has stable DirectML support)
2. Write `core/src/recognizer/ort_parakeet.rs` implementing the
   `Recognizer` trait. Three ONNX sessions (encoder, decoder, joiner)
   loaded once in `ensure_loaded`. Greedy RNN-T decode loop in
   `transcribe`:
   - Run encoder over the full mel-spectrogram (160 hop, 80 mel bins —
     match what sherpa preprocesses today)
   - For each encoder frame: query joiner with current decoder state +
     encoder frame; emit token if not blank; advance decoder state if
     non-blank
   - Detokenize via tokens.txt
   - For TDT specifically: joiner outputs (token, duration) pair —
     duration tells you how many encoder frames to skip ahead. See
     NeMo-toolkit/nemo/collections/asr/modules/rnnt.py for the reference
     algorithm.
3. Cfg-gate the new impl on `target_os` so Mac stays on FluidAudio
4. Bench: same 5-rep harness as TASK-39, threads=8, CPU EP. Should be
   within 10% of TASK-39's 656 ms median. If not, profile.

### Phase 2 — Runtime EP probe

5. EP probe order: TensorRT → CUDA → DirectML → CPU. Each probe is
   "build a Session with this EP; if it succeeds and a 1-second smoke
   decode returns sane output within a timeout (5 s say), keep it.
   Otherwise drop and try the next."
6. Cache the choice at `~/.cache/openwhisper/ep-pref.json` keyed by
   driver/EP versions so subsequent launches skip the probe. Invalidate
   on driver upgrade (compare versions, re-probe if changed).
7. `OPENWHISPER_PROVIDER` env override stays for bench/debug.

### Phase 3 — Bundle + bench

8. Tauri bundler config: include DirectML EP DLLs by default (~30–50 MB).
   Skip CUDA from the default bundle (too heavy) — gate behind a
   "Download GPU pack" UX (out of scope for THIS task; just don't ship
   CUDA in the default).
9. Bench harness updates: same shape as TASK-39, but iterate the EPs
   the host supports, capture vendor-appropriate GPU util:
   - NVIDIA: `nvidia-smi dmon -s u`
   - DirectML on AMD: GPU-Z log or PerfMon `\GPU Engine(*)\Utilization Percentage`
   - DirectML on Intel: same as AMD (DX12 counters)
10. Append all results to `scripts/bench/results/<host>-<date>.txt`.

### Phase 4 — Decision

11. Compare per-EP medians on each available hardware class.
    Decision matrix (mirror TASK-39's frozen-thresholds approach):
    - DirectML decode_ms < 0.8 × CPU decode_ms on majority of hardware
      tested → ship the probe; DirectML wins meaningfully on enough
      boxes to be worth the bundle delta.
    - DirectML decode_ms ≈ CPU decode_ms (±10%) on most hardware →
      ship the probe but log "no benefit detected, staying on CPU"
      for the user. Future-proofs for model upgrades that may benefit.
    - DirectML decode_ms > 1.2 × CPU on most hardware → don't ship the
      probe. Engine swap (ort) still ships because it's a cleaner Rust
      binding, but Windows stays effectively CPU-only. Document and
      close.
12. Decision in `backlog/decisions/recognizer-ort-engine-<YYYY-MM-DD>.md`,
    mirroring `recognizer-cuda-decision-2026-04-26.md`.

## What you should NOT do

- **Don't change the Mac path.** `target_os = "macos"` continues
  FluidAudio via Swift bridge. The `Recognizer` trait abstraction
  exists exactly for this kind of platform divergence.
- **Don't swap the model.** Parakeet-TDT-v3 int8 stays. faster-whisper /
  Whisper-turbo / etc. is a separate, much larger task gated on a fresh
  quality bench. Stay on Parakeet multilingual.
- **Don't ship the EP probe based on RTX 3070 alone.** TASK-39 already
  showed Parakeet doesn't win on this NVIDIA box. The whole point of
  DirectML is cross-vendor — bench AT LEAST one non-NVIDIA box (AMD
  discrete or Intel iGPU) before flipping the probe on. If only one
  box is available, document which classes are unvalidated and gate
  the probe to "opt-in via env" until more data exists.
- **Don't bundle CUDA in the default Tauri build.** TASK-39 measured
  ~1.7 GB floor for the CUDA DLL set. That's a non-starter for a
  default desktop install. CUDA stays as an optional, NVIDIA-targeted
  download (or future "GPU pack" UX), not in the base bundle.
- **Don't skip the bench step.** The "decode_ms dropped + assume GPU
  engaged" trap is real (TASK-39 caught CUDA with a 4% SM utilization
  reality check). Same discipline: vendor-appropriate GPU util sample
  during decode, decoded JSON text identical to baseline.

## Files / pointers

- `core/Cargo.toml` — recognizer feature deps (sherpa-onnx coming out,
  ort going in)
- `core/src/recognizer/sherpa.rs` — TASK-39 era impl; replace
- `core/src/recognizer/mod.rs` — trait + cfg-gated platform default
- `core/src/recognizer/download.rs` — model archive download/extract;
  unchanged
- `apps/tauri/src-tauri/tauri.conf.json` — bundler resource list (where
  the EP DLLs need to be referenced)
- `scripts/bench/bench-sherpa/` — bench harness; extend or fork
- `scripts/bench/smoke-with-wpr.ps1` — Windows wrapper with sampler
- `backlog/decisions/recognizer-cuda-decision-2026-04-26.md` — full
  context on what TASK-39 measured + why ONNX-via-anything underperformed
- Memory: `feedback_ansi_path_marshaling.md` (sherpa C ABI quirk that
  the new impl needs to handle if `ort` calls into C with non-ASCII
  paths — verify or replicate the GetShortPathNameW workaround),
  `feedback_windows_no_admin.md`, `project_parakeet_v3_multilingual_behavior.md`

## Recommended first action

`git log --oneline -5` to see TASK-39's landing commits, then
`cargo build --release -p bench-sherpa` to confirm the existing CPU
path still works on this box. After that: phase 1, in order — engine
swap with CPU EP only, bench against TASK-39's CPU baseline, prove
parity, THEN add EP probe.
```

<!-- SECTION:HANDOVER:END -->
