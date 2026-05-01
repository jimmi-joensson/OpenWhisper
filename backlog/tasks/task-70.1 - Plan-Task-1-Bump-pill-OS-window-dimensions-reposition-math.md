---
id: TASK-70.1
title: 'Plan Task 1: Bump pill OS window dimensions + reposition math'
status: To Do
assignee: []
created_date: '2026-05-01 19:16'
updated_date: '2026-05-01 19:18'
labels:
  - 70-impl
dependencies: []
parent_task_id: TASK-70
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Pill window is 180x110 in tauri.conf.json and tauri.dev.conf.json
- [ ] #2 PILL_WIN_W and PILL_WIN_H in src-tauri/src/lib.rs match the conf values
- [ ] #3 App boots with pill visible and no clipping at new window edges
- [ ] #4 Idle capsule on-screen position matches the pre-change baseline within +/-1 logical pt on Mac and Windows
<!-- AC:END -->
