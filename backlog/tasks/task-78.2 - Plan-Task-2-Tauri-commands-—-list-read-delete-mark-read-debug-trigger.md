---
id: TASK-78.2
title: 'Plan Task 2: Tauri commands — list, read, delete, mark-read, debug-trigger'
status: To Do
assignee: []
created_date: '2026-05-04 06:16'
updated_date: '2026-05-04 08:03'
labels:
  - 78-impl
dependencies: []
parent_task_id: TASK-78
milestone: m-1
ordinal: 36000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Six new Tauri commands wired and reachable from the webview (list, read, delete, delete_all, mark_read, unread_count)
- [ ] #2 state.json persists unread + uploaded_at flags; survives app restart; recreated if missing/corrupt
- [ ] #3 Debug panic-trigger command exists and is gated to debug builds (no release exposure)
- [ ] #4 List/read/delete are idempotent; delete-all is best-effort atomic per file
<!-- AC:END -->
