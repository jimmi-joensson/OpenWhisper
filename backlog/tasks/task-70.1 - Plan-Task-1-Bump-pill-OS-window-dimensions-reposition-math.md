---
id: TASK-70.1
title: 'Plan Task 1: Bump pill OS window dimensions + reposition math'
status: In Review
assignee:
  - '@claude'
created_date: '2026-05-01 19:16'
updated_date: '2026-05-02 08:54'
labels:
  - 70-impl
dependencies: []
parent_task_id: TASK-70
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Pill window is 180x110 in tauri.conf.json and tauri.dev.conf.json
- [x] #2 PILL_WIN_W and PILL_WIN_H in src-tauri/src/lib.rs match the conf values
- [ ] #3 App boots with pill visible and no clipping at new window edges
- [ ] #4 Idle capsule on-screen position matches the pre-change baseline within +/-1 logical pt on Mac and Windows
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
0d2ea0e Window 130x82->180x110 in tauri.conf.json + tauri.dev.conf.json; PILL_WIN_W/H constants in src-tauri/src/lib.rs match; cargo check clean.
<!-- SECTION:NOTES:END -->
