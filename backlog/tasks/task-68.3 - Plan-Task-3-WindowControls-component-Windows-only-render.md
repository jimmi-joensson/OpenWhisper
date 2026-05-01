---
id: TASK-68.3
title: 'Plan Task 3: WindowControls component (Windows-only render)'
status: In Review
assignee: []
created_date: '2026-05-01 16:54'
updated_date: '2026-05-01 17:29'
labels:
  - 68-impl
dependencies: []
parent_task_id: TASK-68
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 WindowControls renders min/max/close on Win32 platform; renders nothing on Mac.
- [ ] #2 Clicking minimize / maximize / close invokes the matching window IPC (verified via shim recording).
- [ ] #3 Maximize glyph (single rounded square) swaps to restore glyph (two overlapping squares) when the window is maximized; subscription via getCurrentWindow().onResized().
- [ ] #4 Close button hover is the Win 11 red (#e81123); other buttons hover at theme grey.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Commit: 620b34e. 66/66 Playwright; tsc clean. Awaiting user QA on Windows for visual.
<!-- SECTION:NOTES:END -->
