---
id: TASK-94.3
title: 'Plan Task 3: openwhisper focus command'
status: In Review
assignee: []
created_date: '2026-11-10T14:40:00Z'
updated_date: '2026-11-10T15:07:00Z'
labels:
  - 94-impl
dependencies: []
parent_task_id: TASK-94
ordinal: 85000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Prints the focus context as text or JSON
- [x] #2 A delay option lets the user switch to another app before the read
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
cli/src/commands/focus.rs. Read time in a cold debug CLI process on macOS: 37 to 71 ms including the Accessibility call.
<!-- SECTION:NOTES:END -->
