---
id: TASK-98.2
title: 'Plan Task 2: Windows capture and paint'
status: In Review
assignee: []
created_date: '2026-11-10T23:24:00Z'
updated_date: '2026-11-11T06:37:00Z'
labels:
  - 98-impl
dependencies: []
parent_task_id: TASK-98
ordinal: 107000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Compiles for x86_64-pc-windows-gnu with tests; rectangle maths and exe-name matching are unit tested on any platform
- [ ] #2 Run on a real Windows machine (open until done there)
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
dca4d9b core/src/vision/windows.rs. Compiles for x86_64-pc-windows-gnu with tests; 9 pure tests run on any platform. A window whose owner cannot be identified while the list is not empty means no screenshot. Thread-level per-monitor DPI awareness for the call. Not run on Windows: AC 2 stays open (parent AC 7). Known: OpenWhisper's own pill and answer panel are in a Windows capture unless the shell marks them excluded from capture.
<!-- SECTION:NOTES:END -->
