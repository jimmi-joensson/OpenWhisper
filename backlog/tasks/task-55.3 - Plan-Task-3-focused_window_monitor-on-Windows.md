---
id: TASK-55.3
title: 'Plan Task 3: focused_window_monitor() on Windows'
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
Add focused_window_monitor to fullscreen/windows.rs by extracting the foreground/skip-list/monitor query from is_fullscreen_now into a shared private helper. Both functions reuse it.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Private helper foreground_monitor_info() extracted; both is_fullscreen_now and focused_window_monitor use it
- [ ] #2 is_fullscreen_now() returns the same value as before for the same inputs
- [ ] #3 focused_window_monitor() returns Some((rcMonitor.left, rcMonitor.top)) for the focused window
- [ ] #4 All four shell-window classes still filtered (Progman, WorkerW, Shell_TrayWnd, Shell_SecondaryTrayWnd)
- [ ] #5 cargo check clean on x86_64-pc-windows-msvc
<!-- AC:END -->
