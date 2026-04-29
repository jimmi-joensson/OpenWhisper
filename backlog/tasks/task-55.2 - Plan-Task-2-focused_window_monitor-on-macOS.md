---
id: TASK-55.2
title: 'Plan Task 2: focused_window_monitor() on macOS'
status: Done
assignee:
  - '@claude'
created_date: '2026-04-29 08:01'
updated_date: '2026-04-29 18:54'
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
- [x] #1 pub fn focused_window_monitor() -> Option<(i32, i32)> exists in fullscreen/mac.rs
- [x] #2 Returns None when AXFocusedApplication, AXFocusedWindow, position, or size queries fail
- [x] #3 Returns Some(origin) matching the focused window's display when AX is granted and a window has focus
- [x] #4 is_fullscreen_now() behavior unchanged
- [x] #5 cargo check clean on aarch64-apple-darwin
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
11365f0 Fullscreen mac: focused_window_monitor() — AX walk + CG display lookup; cargo check clean.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added focused_window_monitor() to fullscreen/mac.rs: extends the existing AX walk with kAXPositionAttribute + kAXSizeAttribute (unpacked via AXValueGetValue), then walks CGDisplay::active_displays() and returns the origin tuple of the display containing the window centre. Returns None on any failure. is_fullscreen_now unchanged. cargo check clean on aarch64-apple-darwin.
<!-- SECTION:FINAL_SUMMARY:END -->
