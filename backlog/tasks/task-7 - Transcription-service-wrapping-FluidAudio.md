---
id: TASK-7
title: Transcription service wrapping FluidAudio
status: In Progress
assignee: []
created_date: '2026-04-22 21:11'
updated_date: '2026-04-23 08:29'
labels:
  - macos
  - model
dependencies:
  - TASK-3
references:
  - 'https://github.com/FluidInference/FluidAudio'
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Swift service layer that wraps FluidAudio and exposes OpenWhisper's internal transcription API. Owns model lifecycle (load/unload on demand), state machine (idle / warming / recording / transcribing / error), streaming callbacks to the pill overlay, and error mapping. Prefers .all compute units (ANE).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Service loads model lazily on first transcription, with progress feedback to UI
- [ ] #2 API exposes transcribe(pcm: Data) async throws -> TranscriptionResult
- [ ] #3 Streaming API emits partial results as audio is processed, for pill animation
- [ ] #4 MLComputeUnits set to .all (ANE preferred) — do not hardcode .cpuOnly or .cpuAndGPU
- [ ] #5 First-token latency under 300ms on M1 for a short utterance (<3s)
- [ ] #6 Model can be unloaded to free memory when app is idle for configurable timeout
<!-- AC:END -->
