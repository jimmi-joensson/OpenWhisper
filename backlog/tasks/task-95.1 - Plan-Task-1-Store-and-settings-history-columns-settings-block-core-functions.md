---
id: TASK-95.1
title: >-
  Plan Task 1: Store and settings - history columns, settings block, core
  functions
status: In Review
assignee: []
created_date: '2026-11-10T14:40:00Z'
updated_date: '2026-11-10T15:28:00Z'
labels:
  - 95-impl
dependencies: []
parent_task_id: TASK-95
ordinal: 86000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Migration adds the original-text and app-name columns; older databases upgrade in place
- [x] #2 history block round-trips through settings.json without clobbering sibling blocks; default off, 30 days
- [x] #3 list, search, get, delete, clear and prune are covered by tests; deleted text is not left readable in the file
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Second version (2026-11-10): the store is a folder of Markdown day files, not SQLite. The user asked for it so other programs, a coding agent above all, can read the history. core/src/history/{mod,tests}.rs: append, list, get, delete, clear, prune; format documented at the top of the module. settings: history block with enabled, retention_days, folder. No schema change in the store (the migration and secure_delete from the first version were removed before any release).
24 unit tests on the format and the rules: exact file output, round trip, escaping of text that looks like structure, hand edits kept, text OpenWhisper did not write never deleted, other file names ignored, Windows line endings, cache invalidation.
AC 1 no longer applies (no migration). AC 3 holds for the functions; 'not left readable in the file' is no longer promised: removed text is cut from a plain file and may survive in backups or sync.
<!-- SECTION:NOTES:END -->
