# Spike: Parakeet TDT 0.6B v2 on Apple Silicon

**Task:** TASK-3
**Date:** 2026-04-23
**Status:** Resolved — pivoting to FluidAudio + FluidInference's pre-converted CoreML artifact.

## Question

How do we run `nvidia/parakeet-tdt-0.6b-v2` inside a Swift/SwiftUI app on macOS, preferring the Apple Neural Engine (ANE) for low power and low thermal cost?

## Answer (TL;DR)

Do **not** hand-roll NeMo → CoreML conversion. Use:

- **[FluidAudio](https://github.com/FluidInference/FluidAudio)** — Apache-2.0 Swift library (SPM/CocoaPods), Swift 6, macOS 14+/iOS 17+. Production-grade — 20+ shipping apps (VoiceInk, Spokenly, macparakeet, etc.).
- **[FluidInference/parakeet-tdt-0.6b-v2-coreml](https://huggingface.co/FluidInference/parakeet-tdt-0.6b-v2-coreml)** — pre-converted CoreML `.mlpackage` artifacts (v3 multilingual also available).
- **ANE execution confirmed.** Memory footprint ~66 MB (vs ~2 GB for MLX/GPU). Throughput ~190× real-time on M4 Pro. Reported WER ~2.5%.

License stack is clean: FluidAudio Apache-2.0 + Parakeet weights CC-BY-4.0 + our code MIT. Attribution bundle: NVIDIA (weights) + FluidInference (conversion + library) + Apple CoreML.

## Paths considered

| Rank | Path | ANE? | Effort | License | Verdict |
|------|------|------|--------|---------|---------|
| 1 | **FluidAudio + FluidInference CoreML artifacts** | **Yes** | Low | Apache-2.0 + CC-BY-4.0 | **Adopt** |
| 2 | swift-parakeet-mlx (Metal GPU) | No (GPU only) | Medium | Apache-2.0 | Fallback only |
| 3 | sherpa-onnx + Parakeet ONNX | No (CPU only) | Medium | Apache-2.0 | Reject — CPU is wasteful |
| 4 | Hand-rolled coremltools conversion (FastConformer encoder + TDT decoder loop in Swift) | Yes | High | OK | Reject — reinventing FluidAudio |
| 5 | parakeet.cpp (ggml) | No | High | MIT | Reject — experimental, decoder not implemented, author paused |

### Why not MLX?

MLX runs on Metal GPU, not ANE. That's ~2 GB RAM vs ~66 MB and materially higher power draw. For an always-on dictation app, ANE is strictly better. MLX is only interesting as a fallback for pre-macOS-14 users, which we don't target in MVP.

### Why not ONNX + CoreML EP?

ONNX Runtime's CoreML Execution Provider cannot compile Parakeet's ops cleanly — see [microsoft/onnxruntime#26355](https://github.com/microsoft/onnxruntime/issues/26355), closed "not planned." Falls back to CPU.

### Why not hand-rolled conversion?

Technically feasible — macOS 15's stateful CoreML support ([Apple docs](https://apple.github.io/coremltools/docs-guides/source/stateful-models.html)) made the TDT decoder tractable. FluidInference already did this work and ships artifacts. Replicating is pointless.

## Architectural implications

1. **Drop `models/` conversion-script task from MVP scope.** `models/` becomes a thin README pointing at the FluidInference HF repo + SHA256 pinning for reproducibility.
2. **TASK-7 (Swift CoreML wrapper) becomes a thin wrapper around FluidAudio**, not a direct CoreML integration. It owns the OpenWhisper-facing API surface: `transcribe(pcm) -> String`, streaming callbacks, state machine, error mapping.
3. **Min macOS target: 14.** FluidAudio requires 14+; Parakeet v3 CoreML artifact specifically needs macOS 14/iOS 17.
4. **Fork strategy.** Single-point-of-failure risk on FluidAudio is low (Apache-2.0, active community, wide adoption) but noted. If it goes unmaintained, we fork. Conversion scripts from FluidInference are also open.
5. **Attribution surface.** About screen + bundled LICENSES.md must credit:
   - NVIDIA — Parakeet weights (CC-BY-4.0)
   - FluidInference / FluidAudio — Apache-2.0 (carry NOTICE if present)
   - Apple — CoreML

## Reference app

[moona3k/macparakeet](https://github.com/moona3k/macparakeet) is a working SwiftUI dictation app using FluidAudio. **GPL-3.0, so we cannot copy code into OpenWhisper**, but it's useful as an architectural reference for how to wire FluidAudio into a pill-overlay dictation flow.

## Next steps

- Re-scope TASK-3: "Integrate FluidAudio + download FluidInference CoreML artifact + ANE smoke test."
- Re-scope TASK-7: "Swift wrapper around FluidAudio exposing OpenWhisper's internal transcription API."
- Keep this doc for posterity — revisit if we ever need multilingual (v3) or non-Mac platforms.

## Sources

- [FluidAudio](https://github.com/FluidInference/FluidAudio)
- [FluidInference Parakeet v2 CoreML](https://huggingface.co/FluidInference/parakeet-tdt-0.6b-v2-coreml)
- [FluidInference Parakeet v3 CoreML](https://huggingface.co/FluidInference/parakeet-tdt-0.6b-v3-coreml)
- [senstella/parakeet-mlx](https://github.com/senstella/parakeet-mlx)
- [FluidInference/swift-parakeet-mlx](https://github.com/FluidInference/swift-parakeet-mlx)
- [ONNX Runtime issue #26355 (Parakeet CoreML failure)](https://github.com/microsoft/onnxruntime/issues/26355)
- [moona3k/macparakeet reference app](https://github.com/moona3k/macparakeet)
- [Apple CoreML Stateful Models guide](https://apple.github.io/coremltools/docs-guides/source/stateful-models.html)
- [macparakeet blog: Whisper vs Parakeet on Neural Engine](https://macparakeet.com/blog/whisper-to-parakeet-neural-engine/)
