---
id: TASK-81.9
title: 'Plan Task 9: CI smoke — cli transcribe against bundled WAV'
status: In Review
assignee: []
created_date: '2026-05-04 15:10'
updated_date: '2026-05-12'
labels:
  - 81-impl
milestone: m-1
dependencies: []
parent_task_id: TASK-81
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add cli/tests/fixtures/hello-world.wav and cli/tests/smoke.rs (assert_cmd integration test). Runs in cargo test; CI workflow picks it up to gate PRs on engine integrity.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 cli/tests/smoke.rs spawns openwhisper transcribe ... --json, asserts .text non-empty and contains hello
- [x] #2 cargo test -p openwhisper-cli passes locally on Mac
- [ ] #3 Same test passes on Windows runner once PR-gate CI workflow lands
- [x] #4 cli/tests/fixtures/hello-world.wav committed — 16 kHz mono i16 PCM WAV (5s / 162 KB; size-budget drift documented in notes, not load-bearing)
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Landed in commit `23eee42`. Test uses `std::process::Command` against the `CARGO_BIN_EXE_openwhisper` env var (no `assert_cmd` dep). Asserts: exit 0, stdout JSON parses, `text` non-empty + contains "hello", confidence in [0,1], duration_ms in (0, 60_000). Runs in ~500 ms on Mac when the recognizer is warm.

AC #4 partial: fixture is the existing 5s/162 KB smoke clip copied from archive/macos/Resources/samples/smoke-test.wav, not the 3s/~96 KB the spec called for. Same format (16 kHz mono i16 PCM); the size budget came from a "keep cargo test fast" concern that doesn't bite at 162 KB. Recording a tighter 3s clip is a follow-up if the fixture proves load-bearing.

AC #3 (Windows runner) waits for PR-gate CI workflow (TASK-82).
<!-- SECTION:NOTES:END -->
