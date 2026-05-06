---
id: TASK-87.2
title: 'Plan Task 2: migration 1 — dictations table + index'
status: To Do
assignee: []
created_date: '2026-05-06 06:09'
labels:
  - 87-impl
dependencies:
  - TASK-87.1
parent_task_id: TASK-87
ordinal: 49000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 core/src/store/migrations.rs exposes migrations() returning Migrations<'static> with the verbatim CREATE TABLE + CREATE INDEX from the spec as migration 1
- [ ] #2 After Store::open_or_init, PRAGMA user_version returns 1 and sqlite_master shows the dictations table (7 columns) + idx_dictations_started_at index
- [ ] #3 Reopen-init on the same file is a no-op: user_version stays 1, no error, no schema change
- [ ] #4 INSERT INTO dictations + SELECT COUNT(*) round-trip works under cargo test
<!-- AC:END -->
