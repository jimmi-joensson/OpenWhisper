---
id: TASK-55.2
title: 'Plan Task 2: focused_window_monitor() on macOS'
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
Extend the AX walk in fullscreen/mac.rs to return the origin of the monitor containing the focused window's center. Use thread-safe Core Graphics display API (CGGetActiveDisplayList + CGDisplayBounds), NOT NSScreen.screens which is main-thread only.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 pub fn focused_window_monitor() -> Option<(i32, i32)> exists in fullscreen/mac.rs
- [ ] #2 Returns None when AXFocusedApplication, AXFocusedWindow, position, or size queries fail
- [ ] #3 Returns Some(origin) matching the focused window's display when AX is granted and a window has focus
- [ ] #4 is_fullscreen_now() behavior unchanged
- [ ] #5 cargo check clean on aarch64-apple-darwin
<!-- AC:END -->
