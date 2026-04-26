---
id: DEC-recognizer-cuda
title: Recognizer CUDA EP — defer (TASK-39 RTX 3070 bench)
status: Frozen
date: 2026-04-26
---

# Recognizer CUDA EP — defer

This doc records the outcome of TASK-39: re-bench the Windows recognizer
path on real consumer hardware (RTX 3070, AMD Ryzen 5 2600) and decide
whether the CUDA EP from the k2-fsa CUDA prebuilt is worth shipping for
NVIDIA users. Full raw numbers in
`scripts/bench/results/DESKTOP-V7KRON6-2026-04-26.txt`.

The TASK-39 spec froze the decision matrix in advance:

| CUDA decode_ms | Action |
|----------------|--------|
| < 100 ms (≥ 50× RT) | ship CUDA variant; scaffold runtime EP detection + Tauri bundling story |
| 100–200 ms | marginal; document, don't ship; defer pending DML on non-NVIDIA hardware |
| ≥ CPU best, or CUDA didn't engage | CUDA path is dead at this sherpa version; document and close |

## Result: defer (close arm 2 in this sherpa/model version)

Best CUDA decode landed at **~925 ms median** (5.47× RT), vs **656 ms
median CPU best** (7.71× RT). CUDA is **41% slower than CPU** on this
machine for this model. SM utilization on the RTX 3070 peaks at **4%**
during decode — basically desktop-idle. The CUDA EP loaded cleanly
(`OrtSessionOptionsAppendExecutionProvider_CUDA` succeeded, no "Fallback
to cpu!" message in stderr) but partitioned only a small fraction of ops
to the GPU. The remaining ops ran on CPU EP under the hood, which we
confirmed by sweeping `OPENWHISPER_NUM_THREADS` under
`OPENWHISPER_PROVIDER=cuda`:

| threads | CUDA decode_ms | CPU decode_ms |
|---------|----------------|---------------|
| 2       | 1236           | 1028          |
| 4       | 1014           |  800          |
| 6       |  971           |  725          |
| 8       |  925           |  656          |

CUDA tracks CPU's curve almost exactly, with a constant ~270 ms
overhead. That's the cost of going through CUDA EP plumbing for a model
that can't saturate the GPU — kernel-launch latency dominates.

This is the same failure pattern the Mac arm hit with CoreML EP
(`backlog/decisions/recognizer-bench-thresholds-2026-04-26.md`):
the EP loaded, no error message fired, but the accelerator stayed idle.
On Mac the cure was to switch off ONNX entirely and use FluidAudio's
hand-tuned `.mlmodelc`. There is no equivalent ANE-class accelerator
abstraction for NVIDIA + ONNX that's known to win for this model
architecture (offline RNN-T transducer, int8-quantized).

## Why the GPU stayed idle

Three contributors, roughly in order of impact:

1. **Model is small for GPU compute.** Parakeet-TDT v3 int8 encoder is
   ~120 M params at int8 ≈ 60 MB on disk. Each frame's worth of work
   doesn't fill an RTX 3070 SM. Kernel launch overhead per op (~5–10 µs
   on Ampere) dominates the actual matmul time on a small batch.
2. **int8 quantization isn't first-class on CUDA EP.** ONNXRuntime CUDA
   EP routes int8-quantized GEMMs through cuBLAS-LT only for specific
   layouts; mismatches fall back to FP32 reference kernels or pop the op
   to CPU EP. The Parakeet int8 quant scheme appears to hit one of those
   slow paths.
3. **Transducer joint network is sequential.** RNN-T decoding is a
   step-loop: emit token → joint network → maybe step encoder. Each
   joint step is tiny and serial — exactly the workload GPUs hate.

Any one of these is enough to lose to CPU. The combination makes it
unwinnable without a different model architecture or quant scheme.

## What about ramming through with TensorRT EP?

The CUDA prebuilt also ships `onnxruntime_providers_tensorrt.dll` (835 KB).
TRT does kernel fusion that can amortize launch overhead. Skipped for
this task because:
- It would require regenerating the int8 calibration tables for TRT's
  int8 path (k2-fsa doesn't ship them) — significant work.
- The same "model is small + transducer is sequential" arguments still
  apply. Best plausible win is shaving the constant overhead, not
  catching CPU.
- We're not measuring batched throughput. Single-utterance latency on
  a ~5 s clip is the only metric the dictation use case cares about.

If a future task explores TRT, frame it as "TRT for streaming Parakeet
zipformer" or "TRT for batched server-side ASR" — not as a fix for the
offline single-clip path TASK-39 measured.

## Bundle-size accounting (the other reason to defer)

The CUDA prebuilt + cuDNN 9.x + CUDA Toolkit 12.x runtime DLLs that
landed in `target/release/` total **2.5 GB** vs ~21 MB for the CPU-only
build. Breakdown of the largest files:

| DLL                                       | Size    |
|-------------------------------------------|---------|
| cudnn_engines_precompiled64_9.dll         | 607 MB  |
| cublasLt64_12.dll                         | 474 MB  |
| cufft64_11.dll                            | 292 MB  |
| cudnn_adv64_9.dll                         | 282 MB  |
| onnxruntime_providers_cuda.dll            | 276 MB  |
| cusparse64_12.dll                         | 276 MB  |
| cudnn_ops64_9.dll                         | 126 MB  |
| cublas64_12.dll                           | 100 MB  |
| (others <60 MB each)                      | ~80 MB  |

Even pruning to the minimum ONNXRuntime CUDA EP needs (cublas, cublasLt,
cudart, cudnn_*, onnxruntime_providers_cuda) the floor is ~1.7 GB. A
3 MB Tauri bundle ballooning to 1.7 GB to deliver a *slower* recognizer
is the wrong tradeoff at any cost.

If a future CUDA arm wins decisively (≥130× RT to clear the same
"competitive" bar Mac uses), the bundling story would be:
- Separate NVIDIA-tagged installer track (don't bloat the default).
- Or runtime download of the CUDA pack on first launch, gated on driver
  detection.
- Or static-link a pruned subset (custom sherpa-onnx build with
  `--use_cuda` + only the cuDNN ops Parakeet touches).

None of these are worth scaffolding now.

## What this changes in the codebase

- `core/src/recognizer/sherpa.rs` num_threads default switched from
  hardcoded `2` to `min(num_cpus::get_physical(), 8)`.
  Reason: on this Ryzen 5 2600 (6c/12t) the sweep showed `threads=8`
  was 36% faster than the prior default of 2; 4-core boxes get `4`,
  ≥ 8-core workstations cap at `8` (avoids ONNXRuntime's per-thread
  coordination cliff observed at threads ≥ logical_cpus on both the
  Xeon RDP and this Ryzen).
  Cost: new dep `num_cpus = "1.16"` (gated on the `recognizer` Cargo
  feature, doesn't pull on Mac swiftui builds).
- `core/Cargo.toml` lists `num_cpus` under `recognizer` feature deps.
- `OPENWHISPER_PROVIDER=cuda` env var still works (sherpa-onnx accepts
  the string and engages the EP) — it's just slower than the default,
  so no Tauri shell change to expose it. Left as a bench knob.

## What this does NOT change

- **No CUDA artifacts in the Tauri bundle.** `target/release/` on this
  box has the CUDA DLLs because we manually pointed
  `SHERPA_ONNX_LIB_DIR` at the merged dir for the bench. Normal builds
  (no env var set) auto-download the CPU shared prebuilt, copy the ~21 MB
  of CPU-only DLLs, and ship that. The Tauri bundle config doesn't need
  to change.
- **No DirectML follow-up scheduled.** k2-fsa still ships no DML
  prebuilt for v1.12.40, so the same source-build constraint that
  blocked DML in the prior bench applies. Revisit if/when k2-fsa
  publishes a DML asset, or when we test a non-NVIDIA Windows box where
  DML would be the only path.
- **No model swap.** The model-architecture argument above (small +
  int8 + sequential transducer) is a property of Parakeet-TDT v3, not a
  bug in this measurement. A different recognizer (Whisper-turbo via
  whisper.cpp's CUDA backend, or a streaming zipformer) would be a
  fresh quality bench, out of scope here.

## Files referenced

- Bench raw: `scripts/bench/results/DESKTOP-V7KRON6-2026-04-26.txt`
- CUDA archive: `C:\sherpa-onnx-archives\sherpa-onnx-v1.12.40-cuda-12.x-cudnn-9.x-win-x64-cuda.tar.bz2`
  (gitignored; can be re-downloaded from k2-fsa releases)
- cuDNN archive: `C:\sherpa-onnx-archives\cudnn-windows-x86_64-9.9.0.52_cuda12-archive.zip`
  (gitignored; from `developer.download.nvidia.com/compute/cudnn/redist/`)
- Default-threads change: `core/src/recognizer/sherpa.rs:90` and
  `core/Cargo.toml`
- Prior decision (Mac CoreML arm): `backlog/decisions/recognizer-bench-thresholds-2026-04-26.md`

## No follow-up task scaffolded

Per the TASK-39 spec, AC#7 only fires if shipping CUDA. We're not
shipping. No new task created. If somebody re-opens this — first action
is to re-test against a future sherpa-onnx version with explicit
TensorRT calibration support, or against a different (larger or
non-transducer) ASR model where GPU saturation is plausible.
