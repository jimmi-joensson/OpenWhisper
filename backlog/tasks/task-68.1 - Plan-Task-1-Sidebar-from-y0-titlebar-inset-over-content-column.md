---
id: TASK-68.1
title: 'Plan Task 1: Sidebar from y=0; titlebar inset over content column'
status: In Review
assignee: []
created_date: '2026-05-01 16:54'
updated_date: '2026-05-01 17:26'
labels:
  - 68-impl
dependencies: []
parent_task_id: TASK-68
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Sidebar's first item renders within the top 60 px of the window.
- [ ] #2 Settings titlebar back-arrow renders at x > 150 (inside content column, not full-width).
- [ ] #3 Mac sidebar has padding-top: 38px via body[data-platform='macos'] selector; non-Mac stays at 14px.
- [ ] #4 Existing main-window + scroll specs pass; one new layout-shape test added and passing.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Commit: 0aee774. 63/63 Playwright; tsc clean. Awaiting user QA.
<!-- SECTION:NOTES:END -->
