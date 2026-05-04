---
id: TASK-63.8
title: 'Plan Task 8: Cleanup settings (core)'
status: To Do
assignee: []
created_date: '2026-04-30 22:26'
updated_date: '2026-05-04 08:03'
labels:
  - 63-impl
dependencies: []
parent_task_id: TASK-63
ordinal: 21000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 CleanupSettings schema added with all four fields and defaults
- [ ] #2 Atomic accessors return current values; reads are non-blocking
- [ ] #3 settings_set_cleanup persists JSON, updates all atomics atomically, and triggers variant reload when applicable
- [ ] #4 Both commands registered in invoke_handler!
<!-- AC:END -->
