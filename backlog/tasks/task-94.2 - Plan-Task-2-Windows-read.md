---
id: TASK-94.2
title: 'Plan Task 2: Windows read'
status: In Review
assignee: []
created_date: '2026-11-10T14:39:00Z'
updated_date: '2026-11-10T15:07:00Z'
labels:
  - 94-impl
dependencies: []
parent_task_id: TASK-94
ordinal: 84000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Foreground window gives exe name, product description and title
- [x] #2 Compiles for x86_64-pc-windows-gnu; unit tests for the helpers
- [ ] #3 Run on a Windows machine before leaving In Review
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
core/src/focus/windows.rs: GetForegroundWindow, QueryFullProcessImageNameW, FileDescription from the exe's version info, GetWindowTextW. Compiles for x86_64-pc-windows-gnu including its three unit tests. NOT run on Windows: ACs 1 and 3 stay open until it is. Known limit: packaged (UWP) apps report ApplicationFrameHost.exe.
<!-- SECTION:NOTES:END -->
