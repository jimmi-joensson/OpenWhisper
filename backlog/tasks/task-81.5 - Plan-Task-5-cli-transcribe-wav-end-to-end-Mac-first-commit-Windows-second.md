---
id: TASK-81.5
title: >-
  Plan Task 5: cli transcribe <wav> end-to-end (Mac first commit, Windows
  second)
status: In Review
assignee: []
created_date: '2026-05-04 15:10'
updated_date: '2026-05-07'
labels:
  - 81-impl
milestone: m-1
dependencies: []
parent_task_id: TASK-81
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement transcribe handler against the stabilized library API. Uses core::prelude only (no private internals). Produces transcript text on stdout; --json gives structured output.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 --json mode emits valid JSON parseable by jq
- [x] #2 Handler uses only core::prelude, no reaching into private modules
- [x] #3 Errors go to stderr; exit code non-zero on failure; no unwrap() in CLI handlers
- [x] #4 Mac path lands in commit 5a; cli transcribe prints non-empty text via FluidAudio
- [x] #5 Windows path lands in commit 5b; if Tauri-state plumbing is needed, follow-up subtask filed before 5b ships
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Mac path landed in commit `14ed19a`. End-to-end smoke against archive/macos/Resources/samples/smoke-test.wav (5s, 16 kHz mono i16) produces "Hello from OpenWhisper, this is a smoke test of parakeet running on the Apple Neural Engine." in 119 ms on M-series ANE, confidence 0.95. Handler reads via hound, gates on 16 kHz mono PCM (i16/i32/f32), bails with an actionable ffmpeg-resample message on mismatched format.

**Windows side validated 2026-05-07:** the same handler runs end-to-end through ort + sherpa-onnx Parakeet on Windows. `cargo test -p openwhisper-cli` (CPU EP) — 1 passed in 11.61s. No separate "commit 5b" was needed: the recognizer trait dispatches FluidAudio (Mac) vs ort+sherpa-onnx (Win) at the core::recognizer layer, and no Tauri-state plumbing turned out to be required. Fixture committal needed a `.gitignore` follow-up (commit `b16c991`) — the original `23eee42` claim that it pinned the .wav was wrong; `.gitignore`'s `!tests/**/fixtures/**` un-ignore was rooted at the repo root and didn't reach `cli/tests/`, so the wav was on the Mac disk but never tracked.
<!-- SECTION:NOTES:END -->
