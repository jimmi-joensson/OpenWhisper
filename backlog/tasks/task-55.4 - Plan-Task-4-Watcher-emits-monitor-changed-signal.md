---
id: TASK-55.4
title: 'Plan Task 4: Watcher emits monitor-changed signal'
status: Done
assignee:
  - '@claude'
created_date: '2026-04-29 08:01'
updated_date: '2026-04-29 18:57'
labels:
  - 55-impl
dependencies: []
parent_task_id: TASK-55
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Extend fullscreen/mod.rs so a single 500 ms poll thread serves both the fullscreen callback and a new pill-follow callback. Gate the new callback on settings::follow_active_screen().
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 One poll thread regardless of which install_* function(s) the caller invokes
- [x] #2 install_pill_follow callback fires on monitor-origin change with Some(origin)
- [x] #3 Callback does NOT fire when focused_window_monitor() returns None
- [x] #4 Callback does NOT fire when settings::follow_active_screen() is false
- [x] #5 Existing fullscreen callback behavior unchanged
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
828e1ce Pill-follow watcher: install_pill_follow + LAST_MONITOR + gated poll tick; cargo check clean on both targets.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Refactored install→install_fullscreen with pub-use alias, added install_pill_follow, single ensure_poller_started gates the spawn. Poll tick checks settings::follow_active_screen() then queries cfg-dispatched focused_window_monitor(); fires MONITOR_CB only on tuple change. LAST_MONITOR not reset on gate-off so re-arming doesn't replay. cargo check clean on both targets.
<!-- SECTION:FINAL_SUMMARY:END -->
