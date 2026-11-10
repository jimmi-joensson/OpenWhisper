---
id: TASK-96.1
title: 'Plan Task 1: Core modes block, resolution, prompt, pipeline, history field'
status: To Do
assignee: []
created_date: '2026-11-10T16:13:00Z'
labels:
  - 96-impl
dependencies: []
parent_task_id: TASK-96
ordinal: 94000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 ModesSettings cleaned keeps ids unique, protects the built-in id and drops dangling app defaults (tests)
- [ ] #2 resolve returns the app default before the selected mode and names the reason (tests)
- [ ] #3 Refine off makes no backend call for any mode (stub test)
- [ ] #4 A custom mode sends its own prompt, model and effort and passes the lenient guard (stub test)
- [ ] #5 Old day files parse as before, entries without a mode render byte for byte as before, the old meta regex matches new entries (tests)
<!-- AC:END -->
