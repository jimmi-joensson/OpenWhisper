---
id: TASK-80
title: Investigate occasional missing words in transcripts — Parakeet-quirk lens
status: To Do
assignee: []
created_date: '2026-05-04 06:06'
updated_date: '2026-05-04 08:03'
labels: []
dependencies: []
priority: medium
ordinal: 34000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
User reports subjective sense that words are occasionally missing from transcripts. No saved repro (no audio + transcript pair to diff). Originally suspected to be coupled to the 2s animation stutter (TASK-79); pre-investigation ruled that out — audio capture is independent of the emitter-thread stall, no samples are dropped on that path. So missing-words is a separate problem.

Likeliest lens: Parakeet-TDT token-level quirks (see openwhisper-parakeet-quirks skill). The skill explicitly notes that what looks like a transcription bug is often a model-level token behavior whose fix is post-processing (custom vocab, DA rules), not engine swaps or audio-routing investigation. Custom-vocab work is tracked separately under TASK-10.

Goal of this task: pin down a concrete repro and classify it. Either (a) it's a known Parakeet quirk → roll into TASK-10's custom-vocab/DA pipeline, or (b) it's something else (audio path, NumPy resample edge case, FFI buffer trim) → file a fresh task with the actual cause.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 READ openwhisper-parakeet-quirks skill before starting; cross-check the failure mode against the listed quirks first
- [ ] #2 Capture at least 3 concrete repros: original audio (16 kHz mono WAV) + observed transcript + expected transcript, saved under backlog/docs/notes/missing-words-repros/<date>-<slug>/
- [ ] #3 Classify each repro: (a) known Parakeet quirk (which one), (b) audio-path issue (start/end clipping, resample, buffer trim), or (c) unknown — needs deeper trace
- [ ] #4 If any repro is class (a): note in TASK-10 with link back to this task; do NOT fix in this task
- [ ] #5 If any repro is class (b) or (c): file a separate fix task with the specific cause; do NOT fix in this task
- [ ] #6 Explicitly out of scope: TASK-79 stutter, recognizer engine swaps, model retraining
<!-- AC:END -->
