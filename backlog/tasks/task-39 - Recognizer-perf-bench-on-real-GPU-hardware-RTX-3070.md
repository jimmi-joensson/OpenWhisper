---
id: TASK-39
title: Recognizer perf bench on real-GPU hardware (RTX 3070)
status: Done
assignee:
  - claude
created_date: '2026-04-26 13:00'
updated_date: '2026-04-26 20:15'
labels:
  - recognizer
  - bench
  - windows
  - gpu
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Re-bench the Windows recognizer path on a real consumer Windows machine with a discrete NVIDIA GPU (RTX 3070, 8 GB Ampere). The current production numbers were collected on a Hyper-V Xeon Platinum 8370C VM exposed via RDP — 4 physical cores, no real GPU, shared memory bandwidth — which understates both the CPU ceiling and makes the GPU EPs untestable.

Two arms:

1. **CPU re-sweep**. Repeat `OPENWHISPER_NUM_THREADS` sweep at 1/2/4/6/8 (wider than the 4-core RDP run) to find the steady-state CPU optimum on this consumer-class CPU. Update the default in `core/src/recognizer/sherpa.rs` if the winner differs from the current default of 2.

2. **CUDA EP drop-in**. k2-fsa publishes `sherpa-onnx-v1.12.40-cuda-12.x-cudnn-9.x-win-x64-cuda.tar.bz2` — a prebuilt sherpa-onnx with CUDA EP linked in. Drop it in via `SHERPA_ONNX_ARCHIVE_DIR`, rebuild bench-sherpa, run with `OPENWHISPER_PROVIDER=cuda`, capture `nvidia-smi` during decode. If GPU utilization rises and decode_ms drops materially, this is the path to ship for NVIDIA users.

Defer DirectML — k2-fsa ships no DML prebuilt for v1.12.40, and building one ourselves needs MSVC + DirectML SDK (heavy lift, blocked under no-admin constraint). With CUDA available as a drop-in for the box at hand, DML can wait until either (a) k2-fsa publishes a DML prebuilt or (b) we have non-NVIDIA hardware to validate the win on.

Outcome decides whether to ship a CUDA-enabled OpenWhisper variant for NVIDIA users (gated on detection at runtime, falls back to CPU otherwise) or to stay CPU-only until DML is available cross-vendor.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria

<!-- AC:BEGIN -->
- [x] #1 num_threads sweep run at 1/2/4/6/8 on RTX 3070 host (extended to 10/12 to confirm peak), results in scripts/bench/results/DESKTOP-V7KRON6-2026-04-26.txt
- [x] #2 Default num_threads changed from hardcoded 2 to `min(num_cpus::get_physical(), 8)` in core/src/recognizer/sherpa.rs (8 won the sweep on this 6c12t Ryzen by 36% over 2; new default produces 6 threads here, validated at 677 ms)
- [x] #3 CUDA tarball placed under C:\sherpa-onnx-archives; used SHERPA_ONNX_LIB_DIR (not SHERPA_ONNX_ARCHIVE_DIR — sherpa-onnx-sys hardcodes the archive name) with a merged lib dir containing CUDA prebuilt DLLs + onnxruntime.lib stub from CPU prebuilt + CUDA Toolkit 12.4 + cuDNN 9.9 runtime DLLs. bench-sherpa rebuilt without errors.
- [x] #4 OPENWHISPER_PROVIDER=cuda decode run; nvidia-smi dmon captured non-zero GPU util (peak 4-8% SM) — CUDA EP loaded successfully, just didn't carry the workload
- [x] #5 CUDA sweep (threads=2/4/6/8) archived in scripts/bench/results/DESKTOP-V7KRON6-2026-04-26.txt under "Arm 2"
- [x] #6 Decision recorded: backlog/decisions/recognizer-cuda-decision-2026-04-26.md — defer (CUDA 41% slower than CPU on this hardware; ~2.5 GB DLL bloat for a regression)
- [x] #7 N/A — not shipping CUDA, no follow-up scaffold needed
<!-- AC:END -->

## Handover prompt

Paste the block below into a fresh Claude Code session on the RTX 3070 machine.

```
# OpenWhisper recognizer perf bench — RTX 3070 follow-up to RDP run

## Goal

Re-bench the Windows recognizer path on real consumer hardware (RTX 3070,
NVIDIA Ampere, 8 GB VRAM) and validate whether the CUDA EP is worth
shipping. This is TASK-39 — see backlog/tasks/task-39 - *.md for the full
spec; this prompt briefs you so you can execute it.

## Context — what's already shipped

- **Project**: OpenWhisper, MIT, local-dictation app. Tauri 2 shell + Rust
  core (`core/`). Mac uses FluidAudio on ANE; Windows uses sherpa-onnx +
  Parakeet-TDT v3 int8. Both behind the `Recognizer` trait in
  `core/src/recognizer/mod.rs`.
- **Recognizer perf state today** (uncommitted at session start? probably
  committed by the time you read this — `git log --oneline -3` to verify):
  - `core/src/recognizer/sherpa.rs` reads `OPENWHISPER_NUM_THREADS` and
    `OPENWHISPER_PROVIDER` from env. Defaults: num_threads=2, provider=coreml
    (Win falls back to CPU). Both are bench knobs added 2026-04-26.
  - `apps/tauri/src-tauri/src/lib.rs` spawns
    `recognizer_ensure_loaded()` from `setup` so the ~2.5s cold load is
    paid invisibly at app boot, not on first Record click.
  - `core/src/recognizer/sherpa.rs` has a Windows-specific short-path
    workaround (GetShortPathNameW) for the `ø` in `JimmiJønsson` —
    sherpa's C ABI uses ANSI/UTF-8 path opens that crash on non-ASCII.
- **Last bench results (2026-04-26, RDP Xeon Platinum 8370C 4-core)**:
  - threads=1 → 431 ms median decode (12× RT)
  - **threads=2 → 326 ms median decode (16× RT) — current default**
  - threads=4 → 333 ms median decode (15× RT) — tied
  - threads=8 → 633 ms median decode (8× RT) — SMT oversubscription
  - cold load_ms steady ~2550 ms across all settings
  - DirectML: blocked at sherpa build level (no DML EP compiled in the
    bundled prebuilt; k2-fsa ships no -directml asset for v1.12.40)
  - All raw numbers in scripts/bench/results/CPC-jj-MMULMHOK-2026-04-26.txt
- **Mac baseline (FluidAudio + ANE)**: ~190× RT (~26 ms for the 5.06 s smoke
  clip). The "competitive" bar for the Windows GPU path is ≥130× RT (= ≤1.5×
  the Mac baseline, the threshold used in the original recognizer
  bench-decision doc).

## What you need to do

### Arm 1 — CPU re-sweep (1, 2, 4, 6, 8 threads)

The RDP box is only 4 physical cores. This box probably has 6-12. Likely the
peak of the curve shifts to 4 or 6 and the magnitude grows beyond 1.32×.

1. `cargo build --release -p bench-sherpa`
2. Run the inline PowerShell loop from the prior session (search the bench
   results file for `OPENWHISPER_NUM_THREADS` to crib the harness — it
   sets the env, calls `target/release/bench-sherpa.exe` 5x, summarizes).
   Use the same smoke clip:
   `apps/macos/Resources/samples/smoke-test.wav` (5.06 s EN, checked-in).
3. Append a sweep block to
   `scripts/bench/results/<COMPUTERNAME>-<YYYY-MM-DD>.txt` mirroring the
   shape of the RDP run.
4. If a value other than 2 wins by >5%, update the default in
   `core/src/recognizer/sherpa.rs:88` (the unwrap_or(2) call) and update
   the comment immediately above it.

### Arm 2 — CUDA EP drop-in

Prebuilt asset (k2-fsa, v1.12.40):
`https://github.com/k2-fsa/sherpa-onnx/releases/download/v1.12.40/sherpa-onnx-v1.12.40-cuda-12.x-cudnn-9.x-win-x64-cuda.tar.bz2`

The Cargo `shared` feature on `sherpa-onnx` runs a build script that
downloads a sherpa-onnx prebuilt at build time. Setting
`SHERPA_ONNX_ARCHIVE_DIR=<dir>` makes it use a local archive instead of
downloading. (Confirmed pattern from the Mac path — see
`backlog/decisions/recognizer-bench-thresholds-2026-04-26.md` "Side-effect"
note.) Steps:

1. Verify NVIDIA driver supports CUDA 12.x: `nvidia-smi` should print a
   driver version ≥525.60 (Linux) / ≥528.33 (Windows). RTX 3070 is fine.
2. Download the cuda tarball above to e.g.
   `C:\sherpa-onnx-archives\` (or any local dir).
3. `$env:SHERPA_ONNX_ARCHIVE_DIR = 'C:\sherpa-onnx-archives'`
4. `cargo clean -p sherpa-onnx-sys` (force the build script to re-run with
   the new archive)
5. `cargo build --release -p bench-sherpa` (will rebuild sherpa-onnx-sys
   against the CUDA prebuilt)
6. Verify the rebuilt artifact pulls in CUDA libs: check
   `target/release/` for `onnxruntime_providers_cuda.dll` or
   `cudart64_*.dll` — if they aren't there, the build script's `shared`
   variant didn't pick the CUDA archive (might need a different env var
   or the archive name has to match a specific pattern; spelunk
   `~/.cargo/registry/src/...sherpa-onnx-sys-*/build.rs` to see what
   patterns it looks for).
7. Run with CUDA: `$env:OPENWHISPER_PROVIDER='cuda'; $env:OPENWHISPER_NUM_THREADS='2'`
   then call `target/release/bench-sherpa.exe <smoke-test.wav>`.
8. Capture stderr — sherpa prints "Fallback to cpu!" if CUDA EP rejects
   the model or if it wasn't compiled in. If you see that message, the
   CUDA prebuilt didn't actually engage; debug with verbose logs
   (`$env:ORT_LOGGING_LEVEL='VERBOSE'`).
9. While decode is running, in a side window: `nvidia-smi dmon -s u -c 30`
   (or just `nvidia-smi -l 1`) to capture GPU utilization. Non-zero
   util% during decode = CUDA actually engaged.
10. If CUDA engaged, run 5 decodes for stable median, append a CUDA block
    to the bench log.

### Arm 3 — Decision

Compare:
- Best CPU number from arm 1 (likely 4 or 6 threads)
- CUDA number from arm 2 (if CUDA engaged)
- Mac baseline 26 ms / 190× RT for context

Decision matrix:
- CUDA decode_ms < 100 ms (= ≥50× RT) → **ship CUDA variant**. File a
  follow-up task for runtime detection (try CUDA, fall back to CPU on
  load failure) + Tauri bundling story (CUDA runtime DLLs add ~150 MB to
  the bundle; consider a separate NVIDIA-tagged release channel).
- CUDA decode_ms 100-200 ms → marginal. Document, don't ship yet, defer
  pending DML evaluation on non-NVIDIA hardware.
- CUDA didn't engage / decode_ms ≥ CPU → CUDA path is dead at this
  sherpa version. Document and close arm 2.

Record the decision in
`backlog/decisions/recognizer-cuda-decision-<YYYY-MM-DD>.md`, mirroring
the format of `backlog/decisions/recognizer-bench-thresholds-2026-04-26.md`.

## What you should NOT do

- Don't change Mac code (this branch is Windows-only). The `Recognizer`
  trait + `cfg(target_os)` switch in `core/src/recognizer/mod.rs` keeps
  the two paths separate; respect that line.
- Don't bump the sherpa-onnx Cargo version. Stay on 1.12.40 — the prior
  Mac bench was against this version, the Tauri shell ships against this
  version, and changing it would invalidate the WER/quality evidence in
  `feedback_parakeet_quirks.md`.
- Don't propose a model swap (Whisper-turbo, etc.) as a perf optimization.
  That's a separate, much larger task gated on a quality re-bench.
- Don't ship CUDA without runtime detection. Naively setting
  `provider=cuda` at boot would crash on machines without NVIDIA drivers.
  If shipping, the Tauri shell needs a probe-and-fall-back step.
- Don't skip the nvidia-smi capture. "decode_ms dropped" without GPU
  utilization evidence is exactly the trap the Mac CoreML smoke fell
  into (low ms, but ANE was 0 mW — turned out the EP loaded but every
  op partitioned to CPU). Verify GPU is doing work.

## Files / pointers

- `core/src/recognizer/sherpa.rs` — sherpa wiring; env knobs at lines 86-95
- `core/src/recognizer/mod.rs` — trait + platform default backend
- `apps/tauri/src-tauri/src/lib.rs` — Tauri shell, recognizer_warmup at boot
- `scripts/bench/bench-sherpa/src/main.rs` — bench runner (cross-platform)
- `scripts/bench/smoke-with-wpr.ps1` — Windows wrapper (Get-Counter sampler)
- `scripts/bench/results/<host>-<date>.txt` — bench archive (gitignored,
  per-host)
- `backlog/decisions/recognizer-bench-thresholds-2026-04-26.md` — full
  context on Mac arm + why FluidAudio is the source-of-truth baseline
- `docs/tauri-port-handover.md` — top-level project context
- Memory: see ~/.claude/projects/.../MEMORY.md, especially
  `feedback_ansi_path_marshaling.md`, `feedback_windows_no_admin.md`,
  `feedback_release_core_in_dev.md`

## Recommended first action

`git log --oneline -10` to confirm what's already committed, then
`pnpm tauri build --debug` (or `cargo build --release -p bench-sherpa`)
to confirm the current toolchain works on this box before changing
anything. After that, arm 1 (CPU re-sweep) before arm 2 — re-establishing
the local CPU baseline gives you the number to compare CUDA against.
```
