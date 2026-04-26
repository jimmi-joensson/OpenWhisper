---
id: DEC-recognizer-bench-thresholds
title: Recognizer bench thresholds (frozen pre-bench, TASK-33)
status: Frozen
date: 2026-04-26
---

# Recognizer bench thresholds — frozen before measuring

This doc records the pass/fail bar for TASK-33 (Tauri Phase 2 recognizer
spike: sherpa-onnx + CoreML EP vs shipped FluidAudio baseline) **before any
numbers are collected**, so the decision is not retro-fitted to whatever
sherpa-onnx happens to do.

Primary path under test: `sherpa-onnx = "1.12.40"` (upstream Rust crate, NOT
the deprecated `thewh1teagle/sherpa-rs`) with `provider: Some("coreml")`,
loading `sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8` (same artifact the
Windows shell uses today).

Baseline: shipped Mac SwiftUI app (`apps/macos/`) running FluidAudio +
FluidInference Parakeet v3 CoreML on ANE. Same machine, same clip, same run.

## Architectural notes locked in before bench

- **Crate pin**: `sherpa-onnx 1.12.40` (Apache-2.0). Default features
  (`static`). No CoreML Cargo feature exists — provider is a runtime string.
  Memory `project_recognizer_tauri.md` to be updated post-task to swap
  "sherpa-rs" → "sherpa-onnx upstream" since the former is deprecated.
- **CoreML compute units**: `OrtSessionOptionsAppendExecutionProvider_CoreML`
  is called with `coreml_flags = 0` per `sherpa-onnx/csrc/session.cc`, which
  resolves to `MLComputeUnits.all`. CoreML decides per-op across CPU / GPU
  (Metal) / ANE. Apple Silicon unified memory means no transfer cost between
  units — GPU usage is not a perf penalty vs ANE for our purposes.
- **Streaming reframe**: AC#2 in TASK-33 says "shell can poll partial
  transcripts." Parakeet-TDT v3 is an offline transducer — sherpa
  `OfflineRecognizer` is batch (accept full waveform → decode → text). No
  real partials. AC#2 reinterpreted: "Tauri shell calls `transcribe(samples)`
  on a worker thread after stop, recognizer returns final text + confidence,
  shell hands result to `dictation_deliver_transcript`." If real streaming
  is needed later, separate task: `OnlineRecognizer` + zipformer model.
- **Module shape**: `core/src/recognizer/` with a `Recognizer` trait and a
  `SherpaParakeet` impl. Trait shape lets us A/B against future
  `WhisperCpp` / `FluidAudioBridge` impls without rewiring the shell. Cost
  is ~10 lines of indirection.
- **Confidence**: sherpa `OfflineResult` does not expose a confidence
  scalar. Return `1.0` placeholder, same as Windows shell does today
  (`apps/windows/OpenWhisper/Dictation/Recognizer.cs:177`).
- **Build profile**: bench harness compiled `--release`. Workspace already
  has `[profile.dev.package.openwhisper-core] opt-level = 3`. Reason
  documented in memory `feedback_rust_release_in_dev_loop.md`.
- **License**: Parakeet weights remain CC-BY-4.0 with NVIDIA attribution
  per memory `project_license.md`. ONNX artifact (`*-int8`) is the same
  weights repackaged by k2-fsa for sherpa-onnx — attribution unchanged.

## Risk: CoreML EP must actually be present in prebuilt onnxruntime

`sherpa-onnx-sys` downloads a prebuilt onnxruntime at build time. If that
prebuilt was compiled without CoreML EP support, `provider: "coreml"`
silently falls back to CPU and the whole bench is invalid. **First action
in TASK-33 is a smoke test**, before curating clips:

1. Run a 10 s clip through the recognizer with `ORT_LOGGING_LEVEL=VERBOSE`.
   Look for `CoreMLExecutionProvider` node assignment in the log. If the
   log says all nodes assigned to `CPUExecutionProvider`, CoreML EP is
   missing or disabled.
2. Run `powermetrics --samplers cpu_power,gpu_power,ane_power -i 250` in a
   side terminal during decode. Either ANE or GPU energy should rise.

If smoke fails, two paths:
- (a) Source-build sherpa-onnx with `--use_coreml`, set `SHERPA_ONNX_LIB_DIR`
  before `cargo build` to skip the prebuilt download. Cost: maybe 1–2 hrs
  cmake + onnxruntime build, plus a checked-in script.
- (b) Skip to fallback (Swift `@_cdecl` FluidAudio staticlib per handover §6).

Decide between (a) and (b) immediately on smoke failure — do not run the
full curated bench against a CPU-only sherpa.

## Frozen pass/fail thresholds

These bind the spike outcome. Numbers come back, threshold says proceed or
fall back. No re-litigation.

| Metric | Pass (proceed sherpa-onnx) | Fallback (Swift @_cdecl FluidAudio staticlib) |
|--------|----------------------------|------------------------------------------------|
| End-to-utterance latency, ~10 s EN clip (cold model already loaded) | ≤ 1.5× FluidAudio baseline | > 1.5× |
| WER, curated ~30 s EN clip vs hand-typed reference | ≤ baseline + 1.0 absolute pts | > baseline + 1.0 pts |
| WER, curated ~30 s DA clip vs hand-typed reference | ≤ baseline + 2.0 absolute pts (DA quirks per `project_parakeet_v3_multilingual_behavior` justify wider bar) | > baseline + 2.0 pts |
| Compute target during decode (`powermetrics ane_power,gpu_power`) | non-zero ANE energy, OR non-zero GPU energy with near-idle CPU | pure CPU (zero ANE + zero GPU delta vs idle) |
| Cold model load time (first call to `recognizer_ensure_loaded`, post-download) | ≤ 5 s | > 10 s (5–10 s = pass-with-note, no fallback by itself) |

First-token-time **dropped** from acceptance criteria — N/A for offline
transducer per "Streaming reframe" above.

If ANY metric falls in the fallback column → scaffold the Swift
`@_cdecl` FluidAudio staticlib path (handover §6), link via
`core/build.rs` shelling `swiftc`. The decision doc is updated with all
measured numbers regardless of outcome — even a clean pass records the
margin so we know how much headroom we have.

## What "baseline" means concretely

Both paths run on the same Mac, same recording, same wall-clock minute
(don't bench Mac path, then unplug, then bench sherpa path — thermal
state matters):

- FluidAudio: small Swift package at `scripts/bench/bench-fluidaudio/`,
  `Package.swift` depending on `FluidAudio` exactly as
  `apps/macos/App/DictationService.swift` does. Loads `AsrModels` v3,
  `AsrManager.transcribe(samples)`. Prints text + ms.
- sherpa-onnx: small Rust binary at `scripts/bench/bench-sherpa/`, depends
  on the new `core` recognizer module. Calls `Recognizer::transcribe`.
  Prints text + ms.
- `scripts/bench/run.sh`: starts `powermetrics` in background → runs
  Swift bench → runs Rust bench → stops powermetrics → computes WER for
  both vs `clips/{en,da}.ref.txt` → appends a numbered row to this file.

WER computed via the `wer` crate (Rust), or hand Levenshtein if dep churn
is annoying. Tokenization: lowercase, strip punctuation `[.,!?;:'"()]`,
collapse whitespace. Same tokenizer applied to both reference and
hypothesis — apples-to-apples.

## Out of scope for this spike

- Streaming / partial transcripts (Parakeet TDT is offline; would need a
  zipformer model + `OnlineRecognizer`).
- Evaluating Whisper-large-v3 or whisper-turbo as alternate model. The
  trait shape leaves room for a follow-up bench but TASK-33 stays
  scoped to Parakeet + sherpa vs Parakeet + FluidAudio.
- Windows side. Windows shell already runs sherpa-onnx in C# at parity
  with what we're proposing for Rust core. Parity there is assumed; this
  spike is about the Mac arm only.

## Result

### Smoke phase (pre-curated-bench)

CoreML EP smoke ran on a 5.06 s clip
(`apps/macos/Resources/samples/smoke-test.wav`) before curating the EN/DA
clips, because the smoke threshold ("non-zero ANE or GPU energy") is the
binary gate that decides whether the curated bench is worth running.

**Finding 1 — sherpa-onnx default static prebuilt has CoreML disabled.**
`cmake/onnxruntime-osx-arm64-static.cmake` last line:
`add_definitions(-DSHERPA_ONNX_DISABLE_COREML)` — the static onnxruntime
build doesn't include the CoreML EP, so sherpa hard-disables the
codepath. Smoke output:
`csrc/session.cc:347 CoreML is for Apple only since onnxruntime>=1.15. Fallback to cpu!`

**Workaround applied**: switched the `sherpa-onnx` Cargo dep to
`default-features = false, features = ["shared"]`. Shared variant pulls
the official Microsoft onnxruntime dylib (1.24.4) which DOES include
CoreML EP. No `SHERPA_ONNX_DISABLE_COREML` define in the shared cmake.

Side-effect: build script's ureq download fails on the GitHub
release-asset 302 → Azure-blob redirect. Workaround: download the
shared archive manually and set
`SHERPA_ONNX_ARCHIVE_DIR=/tmp/sherpa-onnx-archives` (build script picks
up local archive when present).

**Finding 2 — even with shared lib, CoreML EP doesn't accelerate
Parakeet-TDT-v3-int8.** Smoke + powermetrics
(`scripts/bench/smoke-with-powermetrics.sh`):

```
ANE Power: 0 mW (flat the entire window)
GPU Power: 440–475 mW @ 338 MHz (lowest freq bucket, 63% residency)
           → idle/background, no inference
CPU Power: 2.5–8 W, spikes during decode
```

Decode timings on 5.06 s EN clip:
- Static (CoreML hard-disabled, pure CPU): 1075 ms
- Shared (CoreML EP available): cold 1705 ms / warm 1204 ms

CoreML EP being "available" did NOT shift work off CPU. Either every node
assigned to CPU EP at session-init (CoreML EP refused the partition due
to int8 quant + transducer joints), or CoreML EP loaded but inference
silently routed CPU-only. The "Fallback to cpu" warning that fires for
unsupported configs DID NOT appear — so the C side believes CoreML is
on, but powermetrics says otherwise.

**Compute-target threshold result**: pure CPU. Hits the **fallback** row
of the table above ("zero ANE + zero GPU delta vs idle").

**Latency threshold result**: implicit. 1204 ms warm decode for 5.06 s
audio = ~4.2× RT on CPU. Shipped FluidAudio per `project_stt_engine`
memory = ~190× RT, so ~26 ms expected for the same clip on the Mac
baseline. Sherpa+CoreML is ~45× slower than baseline. Threshold was
≤ 1.5× baseline — fail by a wide margin even before curating clips.

### Decision

**Fallback path activated**: scaffold the Swift `@_cdecl` FluidAudio
staticlib per `docs/tauri-port-handover.md` §6. Link into Rust core via
`core/build.rs` shelling `swiftc`. Mac uses `FluidAudioBridge`
(`Recognizer` impl), Windows continues with `SherpaParakeet` (already
works at production-acceptable CPU speed). The trait shape from the
day-one scaffold makes this a clean swap, not a rewrite.

**Skipped**: curated EN+DA clips and the FluidAudio Swift bench arm.
Both were intended to compare sherpa+CoreML against FluidAudio
quantitatively — moot now that the smoke proves sherpa+CoreML doesn't
engage the accelerator at all. Re-introduce when benching a different
candidate (e.g. Whisper-turbo via whisper.cpp).

**Why not chase a source-built sherpa-onnx with `--use_coreml` + FP32
Parakeet (~2.5 GB)**: even if every op converts cleanly to ANE-eligible
ops, FluidInference's hand-tuned `.mlmodelc` artifact is what gets to
~190× RT — generic ONNX→CoreML conversion of the same architecture
won't hit that. Best-case maybe 5–10× RT, still 20–40× off baseline.
Not worth the build-time cost or the larger model bundle vs just
using FluidAudio directly via Swift FFI.

**Cross-platform consequence**: original `project_recognizer_tauri.md`
memory framed "single codepath across OSes" as the reason to fold the
recognizer into Rust core. That partially breaks: Windows uses sherpa
in core, Mac uses FluidAudio via Swift FFI in core. Both hidden behind
the same Rust `Recognizer` trait — call site (Tauri shell) doesn't see
the difference. Update the memory to reflect this once fallback lands.

### Files / state at decision time

- `core/Cargo.toml` carries the `recognizer` Cargo feature with the
  shared sherpa-onnx dep (works on Windows, leaves Mac on the
  yet-to-be-built `FluidAudioBridge`).
- `core/src/recognizer/{mod,sherpa,download}.rs` — trait + sherpa impl
  + model download. The sherpa impl will become Win-only via cfg gating
  once the Mac impl lands.
- `core/examples/recognizer_smoke.rs` — kept as a Win sherpa smoke
  driver. Mac side gets its own smoke driver against the Swift bridge.
- `scripts/bench/{bench-sherpa, bench-fluidaudio}/` — bench-sherpa scaffold
  retained for Win-side use; bench-fluidaudio Package.swift retained as
  starting point for the FluidAudioBridge spike.
- `scripts/bench/smoke-with-powermetrics.sh` — kept as a regression
  guard if we ever revisit sherpa+CoreML.
- 487 MB Parakeet ONNX model cached at
  `~/.cache/openwhisper/models/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/`.
  Win continues to use it; Mac no longer needs it. Don't delete — the
  bench harness still references it for any future re-bench.

