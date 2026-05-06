---
id: TASK-88.3
title: >-
  Plan Task 3: WPM setting — user_wpm field in settings store w/ clamp
  validation
status: To Do
assignee: []
created_date: '2026-05-06 06:15'
labels:
  - 88-impl
dependencies: []
parent_task_id: TASK-88
ordinal: 55000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 user_wpm: u32 field exists in settings JSON store; older files without the key default to 40 via serde default
- [ ] #2 Settings setter clamps writes to [10, 300]: set_user_wpm(5) reads back as 10, set_user_wpm(500) reads back as 300; no error surface
- [ ] #3 React hook exposes userWpm value + setter following the same shape as existing settings hooks
<!-- AC:END -->
