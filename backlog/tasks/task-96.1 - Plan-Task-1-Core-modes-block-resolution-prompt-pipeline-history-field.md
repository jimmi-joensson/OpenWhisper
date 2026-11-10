---
id: TASK-96.1
title: 'Plan Task 1: Core modes block, resolution, prompt, pipeline, history field'
status: In Review
assignee: []
created_date: '2026-11-10T16:13:00Z'
updated_date: '2026-11-10T16:48:00Z'
labels:
  - 96-impl
dependencies: []
parent_task_id: TASK-96
ordinal: 94000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 ModesSettings cleaned keeps ids unique, protects the built-in id and drops dangling app defaults (tests)
- [x] #2 resolve returns the app default before the selected mode and names the reason (tests)
- [x] #3 Refine off makes no backend call for any mode (stub test)
- [x] #4 A custom mode sends its own prompt, model and effort and passes the lenient guard (stub test)
- [x] #5 Old day files parse as before, entries without a mode render byte for byte as before, the old meta regex matches new entries (tests)
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
ccb4bf3 core modes. 349 core lib tests pass (run by the orchestrator). Dev and release builds have separate openwhisper.db files, so migration 5 does not touch the released app's database. Not verified: the live focus read for app defaults (tests fake the frontmost app); the released 0.6.1 app reading a file with an extra line (regex-level test only).
<!-- SECTION:NOTES:END -->
