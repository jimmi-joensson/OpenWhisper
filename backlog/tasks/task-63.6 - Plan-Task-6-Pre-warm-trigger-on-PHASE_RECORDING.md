---
id: TASK-63.6
title: 'Plan Task 6: Pre-warm trigger on PHASE_RECORDING'
status: To Do
assignee: []
created_date: '2026-04-30 22:26'
updated_date: '2026-05-04 08:03'
labels:
  - 63-impl
dependencies: []
parent_task_id: TASK-63
ordinal: 19000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Entering PHASE_RECORDING with cleanup enabled spawns a load task
- [ ] #2 Recording start is not blocked by cleanup load
- [ ] #3 Logs show cleanup model reaching Loaded state during a long recording
- [ ] #4 Cleanup gated on settings::cleanup_enabled() — pre-warm is a no-op when disabled
<!-- AC:END -->
