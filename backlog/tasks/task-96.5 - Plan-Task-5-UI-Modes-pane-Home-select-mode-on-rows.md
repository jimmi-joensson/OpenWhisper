---
id: TASK-96.5
title: 'Plan Task 5: UI Modes pane, Home select, mode on rows'
status: In Review
assignee: []
created_date: '2026-11-10T16:14:00Z'
updated_date: '2026-11-10T17:37:00Z'
labels:
  - 96-impl
dependencies: []
parent_task_id: TASK-96
ordinal: 98000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Modes can be created, renamed, edited and deleted; Dictation is built in and has no delete
- [x] #2 Home select changes the selected mode and is disabled with the reason while Refine is off
- [x] #3 An app from history can be given a default mode and lose it
- [x] #4 A dictation row shows the mode used and why
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
655f14d Modes pane, mode dialog, Home select, mode marker on rows and Refine runs. Hooks on TanStack Query (lib/use-modes.ts); row controls use the shared hover classes. Covered by tests/modes.spec.ts (32 tests). Looks not judged by a person.
<!-- SECTION:NOTES:END -->
