---
id: TASK-65.5
title: 'Plan Task 5: useLastTranscription hook'
status: To Do
assignee: []
created_date: '2026-04-30 22:45'
labels:
  - 65-impl
dependencies: []
parent_task_id: TASK-65
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Hook subscribes to dictation_tick and tracks previous phase via a ref.
- [ ] #2 On phase transition into PHASE_DONE (or PHASE_IDLE from PHASE_TRANSCRIBING) with non-empty trimmed transcript, state updates to {text, timestamp: Date.now(), confidence}.
- [ ] #3 Subsequent finalizations replace state; no list, no growth, no persistence.
- [ ] #4 pnpm tsc --noEmit clean (UI exercise lands in Task 6 — no Playwright assertion this task).
<!-- AC:END -->
