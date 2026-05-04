---
id: decision-3
title: 'Recognizer engine swap to `ort` + DirectML defer (TASK-40)'
date: '2026-04-26 00:00'
status: accepted
---

# Recognizer engine swap — `sherpa-onnx` → `pykeio/ort`

This doc records the outcome of TASK-40: swap the Windows recognizer's
inference engine from sherpa-onnx to the `ort` Rust crate, add a runtime
EP probe (TensorRT → CUDA → DirectML → CPU) for cross-vendor GPU
acceleration, and decide whether DirectML clears CPU on Parakeet-TDT-v3
int8. Companion to `decision-2 - Recognizer CUDA EP defer.md` (TASK-39).
Raw bench numbers in
`scripts/bench/results/DESKTOP-V7KRON6-2026-04-26.txt` (appended below
TASK-39's CUDA arm).

## Decisions

### 1. Engine swap → ship.

`sherpa-onnx` 1.12.40 is removed; `ort` 2.0.0-rc.10 is in. Encoder /
decoder / joiner ONNX sessions are loaded directly through `ort`, and a
~250 LOC Rust greedy RNN-T TDT decoder owns the token-emit loop that
sherpa wrapped in C++ for us. CPU-EP parity confirmed on the RTX 3070
box from TASK-39:

| harness                     | decode_ms (median, 5 reps) | rt_x |
|-----------------------------|-----------------------------|------|
| TASK-39 sherpa-onnx (CPU=8) | 656                         | 7.71×|
| TASK-40 ort     (CPU=8)     | 696                         | 7.27×|

ort is **6% slower** than sherpa-onnx at the same thread count — well
inside the 10% parity budget the spec set. The 6% likely lives in the
Rust mel-preprocessor (rustfft + slaney filterbank in
`core/src/recognizer/mel.rs`) where sherpa used kaldi-native-fbank; not
worth chasing unless a future profile flags it. Transcript text matches
modulo a single-token capitalization (`Engine` vs `engine`) on the
smoke clip — within model-noise drift expected from ε differences in
mel-bin centers between librosa-reference and kaldi-fbank.

Why it had to happen anyway: sherpa wraps DirectML behind a per-vendor
source build per Windows variant (no DML prebuilt on
k2-fsa/sherpa-onnx releases). `ort` exposes DML/CUDA/TRT through one
EP-list API — exactly what we need to ship one Windows binary that
auto-selects the best path per host.

### 2. EP probe → land in code, default to CPU.

The probe lives in `core/src/recognizer/ep_probe.rs`: priority order
TensorRT → CUDA → DirectML → CPU, first one whose encoder ONNX commits
without error wins, choice cached at
`~/.cache/openwhisper/ep-pref.json` keyed by ort version + compiled EP
feature flags. `OPENWHISPER_PROVIDER` env override short-circuits the
probe (matches the TASK-39 knob).

But: the *default* Tauri build does NOT compile in DirectML / CUDA /
TensorRT EPs. With only the `recognizer` feature (no `-directml`),
`compiled_eps()` reports just `["cpu"]`, so the probe immediately picks
CPU. Users / benches who want DML opt in via:

```
cargo build --features openwhisper-core/recognizer-directml
OPENWHISPER_PROVIDER=dml ./bench-sherpa.exe ...
```

### 3. DirectML on RTX 3070 → defer (gate behind opt-in feature).

| EP        | decode_ms median (5 reps) | min  | max  | vs CPU |
|-----------|---------------------------|------|------|--------|
| **CPU**   | **696**                   | 665  | 704  | 1.00×  |
| DirectML  | 1163                      | 1129 | 1197 | 1.67× slower |
| CUDA      | (not in this build — TASK-39: 925, 1.41× slower) | | | |
| TensorRT  | (not in this build) | | | |

DML decode is **67% slower than CPU** on this hardware. nvidia-smi GPU
SM utilization during DML decode peaks at 25% (samples below) — same
"GPU is mostly idle, kernel launch overhead dominates" failure pattern
TASK-39 caught with CUDA EP (peak 4% SM there). Per the TASK-40 frozen
matrix:

> DirectML decode_ms > 1.2 × CPU on most hardware → don't ship the
> probe. Engine swap (ort) still ships because it's a cleaner Rust
> binding, but Windows stays effectively CPU-only.

…with the TASK-40 escape hatch:

> If only one box is available, document which classes are unvalidated
> and gate the probe to "opt-in via env" until more data exists.

Which is what we do: code path exists, `recognizer-directml` Cargo
feature lights it up, but it's neither the Tauri default nor enabled in
the shipped bench binary. **No non-NVIDIA hardware was tested.** The
DML hypothesis (Intel iGPU / AMD discrete might win where NVIDIA's CUDA
core driver path doesn't) is unproven on this box and remains a TODO
for whoever next benches an AMD or Intel-Arc machine.

### 4. Bundle size — same.

Default Tauri Windows build with the engine swap: `bench-sherpa.exe` =
22.3 MB, no extra DLLs in `target/release/`. ort 2.0.0-rc.10 with
`download-binaries` static-links onnxruntime into the binary. DirectML
EP build (`+recognizer-directml`): also 22.3 MB (Δ ≈ 3 KB). DirectML.dll
is shipped by Windows 10 1903+ — we use the system copy. Net: zero
bundle delta from the engine swap, vs the 21 MB sherpa CPU build that
shipped `onnxruntime.dll` + `sherpa-onnx-c-api.dll` next to the exe.

CUDA + TRT EPs are NOT compiled in — that would re-introduce the 1.7 GB
floor TASK-39 already documented. Out of scope.

## Why the GPU stayed idle on DML too

Same three contributors as the CUDA arm in
`decision-2 - Recognizer CUDA EP defer.md`:

1. Parakeet-TDT-v3 int8 encoder (~120 M params @ int8 ≈ 60 MB) is too
   small to saturate the RTX 3070's SMs per encoder frame.
2. int8 quant: DML EP ops route int8 GEMMs through DXIL kernels that
   land back on FP32 reference math when the layout doesn't match —
   measured 25% peak SM (vs CUDA's 4%) suggests DML does *slightly*
   better at fitting the int8 layout, but still not enough to win.
3. The TDT joint network is a step-loop. Each joiner call is tiny
   (joiner.int8.onnx is 6 MB). Kernel launch overhead per joint step
   dwarfs the GEMM time on a small batch.

What changes the picture: a non-NVIDIA box where DML is the *only* GPU
path the driver knows about — Intel Arc / Iris Xe iGPUs especially.
Those don't have a CUDA back door, so DML's path is effectively the
only "GPU compute on this box" option, and even modest acceleration
beats CPU. Re-bench when such hardware shows up.

## What this changes in the codebase

- **NEW** `core/src/recognizer/ort_parakeet.rs` — Recognizer impl using
  `ort` 2.x. Loads encoder / decoder / joiner sessions, runs the greedy
  TDT decode loop (see TASK-40 spec for I/O reference). Replaces the
  sherpa-onnx wrapper.
- **NEW** `core/src/recognizer/mel.rs` — librosa-style 128-bin slaney
  mel preprocessor (Parakeet 0.6B v3 expects 128 mel bins, hop 160,
  win 400, n_fft 512, slaney filterbank, per-feature normalize, log
  guard 2^-24). Pure Rust on rustfft. Unit tests covering mel<->Hz
  round-trip + filterbank shape + extract output.
- **NEW** `core/src/recognizer/ep_probe.rs` — runtime EP resolver
  (env override → cache → live probe), per-host cache file.
- **DELETED** `core/src/recognizer/sherpa.rs` — the sherpa-onnx
  wrapper. `sherpa-onnx-sys` is no longer in the Windows build closure
  (AC#1).
- **MODIFIED** `core/Cargo.toml` — `recognizer` feature now pulls
  `ort = "=2.0.0-rc.10"` + `rustfft` + `ndarray` instead of
  `sherpa-onnx`. New opt-in EP features: `recognizer-directml`,
  `recognizer-cuda`, `recognizer-tensorrt`. `num_cpus` retained for the
  TASK-39 thread heuristic.
- **MODIFIED** `core/src/recognizer/mod.rs` — `OrtParakeet` is the
  non-Mac default backend. Mac path (`FluidAudioBridge`) untouched.
- **MODIFIED** `scripts/bench/bench-sherpa/src/main.rs` — JSON output
  now reports `engine: "ort"` and `provider: "<ep>"`. Binary name kept
  to avoid script churn.
- **NEW** `scripts/bench/bench-eps.ps1` — multi-EP bench script
  (CPU + whatever EPs the build enabled). Captures nvidia-smi during
  GPU runs. Appends to `<host>-<date>.txt` mirroring the TASK-39
  format.

## What this does NOT change

- **Mac path**: `target_os = "macos"` continues to use FluidAudio via
  the Swift bridge. The `Recognizer` trait and `recognizer/mod.rs`
  cfg-gating are unchanged. (AC#9 — verified by reading
  `core/src/recognizer/mod.rs`; FluidAudio import + default backend
  still gated on `target_os = "macos"`. A Mac re-build was not run on
  this Windows box; the Swift FFI surface didn't move.)
- **Model**: Parakeet-TDT-v3 0.6B int8 stays. faster-whisper /
  whisper.cpp / streaming zipformer are separate quality benches —
  out of scope here.
- **Default Tauri bundle**: still ships `recognizer` + CPU EP only.
  No DML / CUDA / TRT in the default. `recognizer-directml` is opt-in
  for now (Cargo feature on the bench harness only) — flip it in the
  Tauri Cargo.toml once a non-NVIDIA box validates DML wins.
- **`OPENWHISPER_PROVIDER`** env knob: kept (TASK-39 had it for sherpa,
  TASK-40 keeps it for ort). Bench/dev override for the EP probe.
- **`OPENWHISPER_NUM_THREADS`** env knob: kept. CPU EP intra-op
  thread count, defaults to `min(num_cpus::get_physical(), 8)`.

## Files referenced

- Bench raw: `scripts/bench/results/DESKTOP-V7KRON6-2026-04-26.txt`
  (TASK-40 arm appended below TASK-39's CUDA arm)
- ort 2.0.0-rc.10: https://github.com/pykeio/ort
- Prior decision (TASK-39 CUDA arm):
  `backlog/decisions/decision-2 - Recognizer CUDA EP defer.md`
- Prior decision (Mac CoreML arm):
  `backlog/decisions/decision-1 - Recognizer bench thresholds.md`

## Follow-up tasks (NOT scaffolded — file when needed)

1. **Re-bench DirectML on non-NVIDIA hardware.** Either AMD discrete
   or Intel Arc / Iris Xe iGPU. If DML clears CPU on those classes,
   flip `recognizer-directml` on by default in the Tauri build and
   ship the probe.
2. **Reflect-pad / mel-drift investigation.** The single-token text
   drift (`Engine` vs `engine` on the smoke clip) likely lives in the
   mel preprocessor. Compare frame-by-frame against
   `kaldi-native-fbank`'s output on a fixed wav and tune the
   reflect-padding / preemph order to match. Optional — outputs are
   semantically equivalent.
3. **Tauri bundle config audit.** Confirm `cargo build --release` for
   `openwhisper-tauri` on Windows really produces a self-contained
   exe (no onnxruntime.dll dependency). Document in
   `apps/tauri/README.md` so the next person doesn't have to re-derive
   the bundle story.
