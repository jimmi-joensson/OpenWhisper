---
id: TASK-68.4
title: 'Plan Task 4: Drop Windows OS chrome via Rust setup()'
status: In Review
assignee: []
created_date: '2026-05-01 16:54'
updated_date: '2026-05-01 17:30'
labels:
  - 68-impl
dependencies: []
parent_task_id: TASK-68
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 lib.rs::setup() calls set_decorations(false) only on Windows.
- [ ] #2 Smoke on Win 11: no OS title bar; Aero-snap (Win+arrows) works; rounded corners present.
- [ ] #3 Smoke on Mac: behavior unchanged from pre-task — traffic-lights, drag, sidebar continuity all still right.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Commit: 706bced. cargo check clean. Awaiting Win 11 smoke (no OS title bar, Aero-snap, rounded corners) + Mac smoke (Overlay traffic-lights unchanged).
<!-- SECTION:NOTES:END -->
