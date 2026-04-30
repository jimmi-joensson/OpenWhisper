---
id: TASK-63.5
title: 'Plan Task 5: Wrap CleanupEngine in ModelHandle (uses TASK-62)'
status: To Do
assignee: []
created_date: '2026-04-30 22:26'
labels:
  - 63-impl
dependencies: []
parent_task_id: TASK-63
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Cleanup model loaded via ModelHandle with 60-s idle timeout
- [ ] #2 cleanup_process(text, hint) auto-loads if Unloaded, runs the engine, returns to Loaded with idle re-armed
- [ ] #3 Variant switch triggers unload + new loader closure (no stale model)
- [ ] #4 Handle registered with lifecycle registry — Keep models warm setting affects it
<!-- AC:END -->
