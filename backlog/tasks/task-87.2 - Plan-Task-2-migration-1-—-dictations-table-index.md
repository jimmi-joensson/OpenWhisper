---
id: TASK-87.2
title: 'Plan Task 2: migration 1 — dictations table + index'
status: Done
assignee:
  - '@claude'
created_date: '2026-05-06 06:09'
updated_date: '2026-05-06 19:02'
labels:
  - 87-impl
dependencies:
  - TASK-87.1
parent_task_id: TASK-87
ordinal: 49000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 core/src/store/migrations.rs exposes migrations() returning Migrations<'static> with the verbatim CREATE TABLE + CREATE INDEX from the spec as migration 1
- [x] #2 After Store::open_or_init, PRAGMA user_version returns 1 and sqlite_master shows the dictations table (7 columns) + idx_dictations_started_at index
- [x] #3 Reopen-init on the same file is a no-op: user_version stays 1, no error, no schema change
- [x] #4 INSERT INTO dictations + SELECT COUNT(*) round-trip works under cargo test
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Commit: bf3d847. cargo test -p openwhisper-core --lib store:: → 5/5. Awaiting code review; user-visible verify deferred to TASK-87.3.

bf3d847 TASK-87.2: migration 1 + 5 store tests green
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Merged via PR #16 (squash 2415f3a). Cross-platform smoke green: Mac + Windows.
<!-- SECTION:FINAL_SUMMARY:END -->
