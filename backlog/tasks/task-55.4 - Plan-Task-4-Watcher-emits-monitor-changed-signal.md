---
id: TASK-55.4
title: 'Plan Task 4: Watcher emits monitor-changed signal'
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
Extend fullscreen/mod.rs so a single 500 ms poll thread serves both the fullscreen callback and a new pill-follow callback. Gate the new callback on settings::follow_active_screen().
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 One poll thread regardless of which install_* function(s) the caller invokes
- [ ] #2 install_pill_follow callback fires on monitor-origin change with Some(origin)
- [ ] #3 Callback does NOT fire when focused_window_monitor() returns None
- [ ] #4 Callback does NOT fire when settings::follow_active_screen() is false
- [ ] #5 Existing fullscreen callback behavior unchanged
<!-- AC:END -->
