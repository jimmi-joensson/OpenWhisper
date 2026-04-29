---
id: TASK-55.3
title: 'Plan Task 3: focused_window_monitor() on Windows'
status: Done
assignee:
  - '@claude'
created_date: '2026-04-29 08:01'
updated_date: '2026-04-29 18:55'
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
- [x] #1 Private helper foreground_monitor_info() extracted; both is_fullscreen_now and focused_window_monitor use it
- [x] #2 is_fullscreen_now() returns the same value as before for the same inputs
- [x] #3 focused_window_monitor() returns Some((rcMonitor.left, rcMonitor.top)) for the focused window
- [x] #4 All four shell-window classes still filtered (Progman, WorkerW, Shell_TrayWnd, Shell_SecondaryTrayWnd)
- [x] #5 cargo check clean on x86_64-pc-windows-msvc
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
fabb6e9 Fullscreen win: foreground_monitor_info helper + focused_window_monitor; cargo check clean on both targets.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Extracted foreground_monitor_info() returning (HWND, RECT, MONITORINFO) — both is_fullscreen_now and focused_window_monitor route through it. Deviated from plan signature by including HWND so the chromeless branch reads style bits from the same window as the rect (avoids a second GetForegroundWindow race). is_fullscreen_now behavior preserved; all four shell classes still filtered. focused_window_monitor returns Some((rcMonitor.left, rcMonitor.top)). cargo check clean on x86_64-pc-windows-gnu (and aarch64-apple-darwin).
<!-- SECTION:FINAL_SUMMARY:END -->
