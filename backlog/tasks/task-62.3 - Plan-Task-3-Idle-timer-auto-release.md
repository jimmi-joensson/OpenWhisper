---
id: TASK-62.3
title: 'Plan Task 3: Idle timer + auto-release'
status: To Do
assignee: []
created_date: '2026-04-30 22:25'
updated_date: '2026-05-04 08:03'
labels:
  - 62-impl
dependencies: []
parent_task_id: TASK-62
ordinal: 6000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 with_idle_timeout constructor and set_idle_timeout setter exist on ModelHandle
- [ ] #2 After idle expires, handle transitions Loaded->Unloaded automatically
- [ ] #3 Active calls cancel the timer; it re-arms on return to Loaded
- [ ] #4 Duration::MAX (the keep-warm path) keeps the model resident indefinitely
- [ ] #5 Tokio runtime requirement documented in module docs
<!-- AC:END -->
