---
id: TASK-21
title: 'Windows port spike: sherpa-onnx + Parakeet ONNX on Win11 VM'
status: Done
assignee: []
created_date: '2026-04-24 06:07'
updated_date: '2026-04-30 16:31'
labels:
  - windows
  - stt
dependencies:
  - TASK-7
references:
  - 'https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx'
  - 'https://github.com/k2-fsa/sherpa-onnx'
  - 'https://github.com/amd/RyzenAI-SW/tree/main/Demos/ASR/Parakeet-TDT'
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
First cross-vendor Windows spike. Goal: validate Parakeet-TDT v3 transcribes on an arbitrary Win11 machine (no NVIDIA/AMD-specific assumptions) via sherpa-onnx + community ONNX weights from istupakov. Runs in UTM ARM64 VM on Apple Silicon for dev iteration — real x64 perf numbers come later from a real Windows box. Unblocks sketching the SpeechRecognizer trait (TASK-22) against a concrete Rust API rather than speculating.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Win11 ARM VM boots in UTM w/ Python 3.11 + sherpa-onnx prebuilt
- [ ] #2 Parakeet-TDT v3 ONNX transcribes a test WAV file end-to-end
- [ ] #3 CPU EP works (baseline, required for no-GPU laptops)
- [ ] #4 DirectML EP attempted (opt-in, any DX12 GPU path)
- [ ] #5 WER + latency captured vs CoreML/FluidAudio reference on same utterance
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Finish UTM Win11 setup (in progress). 2. Install Python 3.11 + venv + pip install onnxruntime sherpa-onnx. 3. Pull istupakov v3 ONNX weights. 4. Run sherpa-onnx CLI on samples/ WAV, record output + time. 5. Try DirectML EP via onnxruntime-directml. 6. Document numbers in final summary — input for TASK-22 trait design.
<!-- SECTION:PLAN:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Closed during 2026-04-30 backlog review. Spike served — sherpa-onnx + Parakeet ONNX shipped on Windows in v0.4.0 via TASK-33 (recognizer spike) and TASK-40 (ort engine swap). Decisions captured in backlog/decisions/recognizer-{cuda,ort}-*.md.
<!-- SECTION:FINAL_SUMMARY:END -->
