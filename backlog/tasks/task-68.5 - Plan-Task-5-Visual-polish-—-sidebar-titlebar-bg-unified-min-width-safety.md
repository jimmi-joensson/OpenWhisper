---
id: TASK-68.5
title: 'Plan Task 5: Visual polish — sidebar/titlebar bg unified, min-width safety'
status: In Review
assignee: []
created_date: '2026-05-01 16:55'
updated_date: '2026-05-01 17:30'
labels:
  - 68-impl
dependencies: []
parent_task_id: TASK-68
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Sidebar and titlebar share the same computed background color (rgb overlay over --background).
- [ ] #2 Existing scroll spec at 600x500 still passes (no overflow regression).
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Commit: 33a5e17. 66/66 Playwright; tsc clean.
<!-- SECTION:NOTES:END -->
