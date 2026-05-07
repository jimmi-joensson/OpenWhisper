---
id: TASK-78.2
title: 'Plan Task 2: Tauri commands — list, read, delete, mark-read, debug-trigger'
status: Done
assignee:
  - '@claude'
created_date: '2026-05-04 06:16'
updated_date: '2026-05-07 22:22'
labels:
  - 78-impl
milestone: m-1
dependencies: []
parent_task_id: TASK-78
ordinal: 36000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Six new Tauri commands wired and reachable from the webview (list, read, delete, delete_all, mark_read, unread_count)
- [x] #2 state.json persists unread + uploaded_at flags; survives app restart; recreated if missing/corrupt
- [x] #3 Debug panic-trigger command exists and is gated to debug builds (no release exposure)
- [x] #4 List/read/delete are idempotent; delete-all is best-effort atomic per file
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
b43c0f8 list/read/delete/delete_all/mark_read/unread_count + debug-trigger; state.json with default-true unread
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Six webview commands wired: crashes_list, crashes_read, crashes_delete, crashes_delete_all, crashes_mark_read, crashes_unread_count, plus crashes_debug_trigger_panic. state.json (atomic-on-rename) tracks per-crash unread + uploaded_at; missing → default empty, corrupt → reset; new crashes default unread via hand-rolled Default impl. Delete + mark_read are idempotent (NotFound = success). Debug trigger gated to debug + feature "dev-panic"; release build replaces it with an Err stub so the handler stays registered but panic surface is removed. 13 unit tests + full Tauri suite green; release build clean. Commit b43c0f8.
<!-- SECTION:FINAL_SUMMARY:END -->
