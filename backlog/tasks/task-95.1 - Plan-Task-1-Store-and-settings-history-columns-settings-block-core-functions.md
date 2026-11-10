---
id: TASK-95.1
title: >-
  Plan Task 1: Store and settings - history columns, settings block, core
  functions
status: In Review
assignee: []
created_date: '2026-11-10T14:40:00Z'
updated_date: '2026-11-10T15:07:00Z'
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
Migration 5 (raw_transcript, app_name), PRAGMA secure_delete, core/src/history, core/src/settings/history.rs. A test reads the database file after a clear and fails without secure_delete (checked by switching it off once). A real v4 database from the production app upgraded in place with its 1,015 rows and word total unchanged.
<!-- SECTION:NOTES:END -->
