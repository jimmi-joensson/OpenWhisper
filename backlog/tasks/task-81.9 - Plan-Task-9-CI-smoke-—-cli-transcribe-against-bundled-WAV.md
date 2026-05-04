---
id: TASK-81.9
title: 'Plan Task 9: CI smoke — cli transcribe against bundled WAV'
status: To Do
assignee: []
created_date: '2026-05-04 15:10'
updated_date: '2026-05-04 15:17'
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
- [ ] #1 cli/tests/smoke.rs spawns openwhisper transcribe ... --json, asserts .text non-empty and contains hello
- [ ] #2 cargo test -p openwhisper-cli passes locally on Mac
- [ ] #3 Same test passes on Windows runner once PR-gate CI workflow lands
- [ ] #4 cli/tests/fixtures/hello-world.wav committed — 16 kHz mono i16 PCM WAV, ~3s, ~96 KB
<!-- AC:END -->
