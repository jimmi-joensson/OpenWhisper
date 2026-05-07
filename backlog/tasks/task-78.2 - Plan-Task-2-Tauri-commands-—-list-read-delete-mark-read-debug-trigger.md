---
id: TASK-78.2
title: 'Plan Task 2: Tauri commands — list, read, delete, mark-read, debug-trigger'
status: In Progress
assignee:
  - '@claude'
created_date: '2026-05-04 06:16'
updated_date: '2026-05-07 16:46'
labels:
  - 78-impl
milestone: m-1
dependencies: []
parent_task_id: TASK-78
ordinal: 36000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Six new Tauri commands wired and reachable from the webview (list, read, delete, delete_all, mark_read, unread_count)
- [ ] #2 state.json persists unread + uploaded_at flags; survives app restart; recreated if missing/corrupt
- [ ] #3 Debug panic-trigger command exists and is gated to debug builds (no release exposure)
- [ ] #4 List/read/delete are idempotent; delete-all is best-effort atomic per file
<!-- AC:END -->
