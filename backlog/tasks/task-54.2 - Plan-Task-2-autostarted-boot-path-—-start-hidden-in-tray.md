---
id: TASK-54.2
title: 'Plan Task 2: --autostarted boot path — start hidden in tray'
status: To Do
assignee: []
created_date: '2026-04-29 17:44'
labels:
  - 54-impl
dependencies: []
parent_task_id: TASK-54
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 setup() parses --autostarted from std::env::args and records the boolean
- [ ] #2 When the flag is present, the main window is not shown / focused on boot; tray + pill remain functional
- [ ] #3 When the flag is absent, the existing boot behavior is unchanged
- [ ] #4 Manual smoke confirms both boot paths
<!-- AC:END -->
