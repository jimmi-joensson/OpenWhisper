---
id: TASK-62.7
title: 'Plan Task 7: Tauri telemetry commands + state-change events'
status: To Do
assignee: []
created_date: '2026-04-30 22:25'
labels:
  - 62-impl
dependencies: []
parent_task_id: TASK-62
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 telemetry_get_memory Tauri command returns MemoryStats with per-model rows
- [ ] #2 model-state-changed event fires on every Lifecycle transition with { label, state }
- [ ] #3 Event includes both recognizer transitions and any future cleanup transitions
- [ ] #4 cargo check clean
<!-- AC:END -->
