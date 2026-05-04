---
id: TASK-63.7
title: 'Plan Task 7: Wire cleanup into transcript pipeline'
status: To Do
assignee: []
created_date: '2026-04-30 22:26'
updated_date: '2026-05-04 08:03'
labels:
  - 63-impl
dependencies: []
parent_task_id: TASK-63
ordinal: 20000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Cleanup runs after transcript::process in both Mac and Tauri shells
- [ ] #2 Cleanup is gated on settings::cleanup_enabled() and falls back to rule-pass output on any error
- [ ] #3 Same source of truth: cleanup::cleanup_process called from both shells; no duplicated logic
- [ ] #4 Manual smoke confirms aggressive-on vs aggressive-off behavior
<!-- AC:END -->
