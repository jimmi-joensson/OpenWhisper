---
id: TASK-55.5
title: 'Plan Task 5: Reposition pill on monitor change + boot wiring'
status: In Progress
assignee:
  - '@claude'
created_date: '2026-04-29 08:01'
updated_date: '2026-04-29 19:02'
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
- [x] #4 reposition_pill registered in invoke_handler!; PillOverlay.tsx invoke updated
- [x] #5 Old position_pill_bottom_center symbol removed
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
1a12758 reposition_pill + place_pill helper + find_tauri_monitor (mac/win) + watcher callback in setup + alias removed + PillOverlay invoke renamed.
ACs #1-3 require multi-monitor mac smoke (deferred to manual verification per plan).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
reposition_pill takes optional monitor_origin; on Some dispatches to fullscreen::find_tauri_monitor (cfg-routed). mac side converts Tauri monitor position from physical-px to logical points; win compares direct. place_pill helper called from both the command and the watcher callback, both wrapped in app.run_on_main_thread. install→install_fullscreen alias removed; pill-follow callback registered alongside. Frontend invoke renamed and passes { monitor_origin: null }. cargo check clean both targets, tsc clean. ACs #1-3 require multi-monitor mac smoke (handover-flagged manual step).
<!-- SECTION:FINAL_SUMMARY:END -->
