---
id: TASK-95.4
title: 'Plan Task 4: Tauri commands'
status: In Review
assignee: []
created_date: '2026-11-10T14:40:00Z'
updated_date: '2026-11-10T15:07:00Z'
labels:
  - 95-impl
dependencies: []
parent_task_id: TASK-95
ordinal: 89000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 history_get, history_set, history_list, history_delete and history_clear are thin wrappers over core
- [x] #2 The webview is told when history changes
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
apps/tauri/src-tauri/src/history.rs: history_get, history_set, history_list, history_delete, history_clear; history::init hydrates the switch and applies retention at boot. The existing stats_changed event is the change signal.
<!-- SECTION:NOTES:END -->
