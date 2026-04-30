---
id: TASK-62.4
title: 'Plan Task 4: Performance settings (keep_models_warm)'
status: To Do
assignee: []
created_date: '2026-04-30 22:25'
labels:
  - 62-impl
dependencies: []
parent_task_id: TASK-62
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 PerformanceSettings schema added; keep_models_warm defaults false
- [ ] #2 settings_set_keep_models_warm(true) persists JSON, flips atomic, and reconfigures every registered ModelHandle's timer in the same call
- [ ] #3 Registered handles correctly update on flip without app restart
- [ ] #4 Both Tauri commands wired in invoke_handler!
- [ ] #5 cargo check clean
<!-- AC:END -->
