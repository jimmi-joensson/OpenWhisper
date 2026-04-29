---
id: TASK-55.5
title: 'Plan Task 5: Reposition pill on monitor change + boot wiring'
status: To Do
assignee: []
created_date: '2026-04-29 08:01'
labels:
  - 55-impl
dependencies: []
parent_task_id: TASK-55
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Rename position_pill_bottom_center to reposition_pill, accept an optional monitor-origin hint, and register the watcher callback in setup() so the pill jumps to the new monitor within ~500 ms of foreground change.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Pill repositions to bottom-center of new monitor within ~500 ms of foreground switch (multi-monitor smoke)
- [ ] #2 Recording-in-progress pill follows without disrupting the SVG tween or level meter
- [ ] #3 Single-monitor: zero set_position calls beyond the boot placement
- [ ] #4 reposition_pill registered in invoke_handler!; PillOverlay.tsx invoke updated
- [ ] #5 Old position_pill_bottom_center symbol removed
<!-- AC:END -->
