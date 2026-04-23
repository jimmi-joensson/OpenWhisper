---
id: TASK-7
title: Transcription service wrapping FluidAudio
status: Done
assignee: []
created_date: '2026-04-22 21:11'
updated_date: '2026-04-23 18:16'
labels:
  - macos
  - model
dependencies:
  - TASK-3
references:
  - 'https://github.com/FluidInference/FluidAudio'
priority: high
ordinal: 1000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Swift service layer that wraps FluidAudio and exposes OpenWhisper's internal transcription API. Owns model lifecycle (load/unload on demand), state machine (idle / warming / recording / transcribing / error), streaming callbacks to the pill overlay, and error mapping. Prefers .all compute units (ANE).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Service loads model lazily on first transcription, with progress feedback to UI
- [x] #2 API exposes transcribe(pcm: Data) async throws -> TranscriptionResult
- [x] #3 Streaming API emits partial results as audio is processed, for pill animation
- [x] #4 MLComputeUnits set to .all (ANE preferred) — do not hardcode .cpuOnly or .cpuAndGPU
- [x] #5 First-token latency under 300ms on M1 for a short utterance (<3s)
- [x] #6 Model can be unloaded to free memory when app is idle for configurable timeout
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Refactored ContentView (~300 lines of tangled state + logic) into a DictationService @Observable class owning the full pipeline: model lifecycle, mic capture, state machine (Phase enum: idle/loadingModel/recording/transcribing/done/error), timer, level history, pill updates, and text injection. ContentView dropped to ~170 lines of view code observing the service. No behaviour change. Unblocks TASK-9 (first-run download UX), TASK-10 (custom vocabulary), TASK-11 (settings).
<!-- SECTION:FINAL_SUMMARY:END -->
