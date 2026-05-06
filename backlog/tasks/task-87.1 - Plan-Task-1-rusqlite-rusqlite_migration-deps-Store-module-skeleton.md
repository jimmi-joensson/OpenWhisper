---
id: TASK-87.1
title: 'Plan Task 1: rusqlite + rusqlite_migration deps + Store module skeleton'
status: In Progress
assignee:
  - '@claude'
created_date: '2026-05-06 06:09'
updated_date: '2026-05-06 06:57'
labels:
  - 87-impl
dependencies: []
parent_task_id: TASK-87
ordinal: 48000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Store struct exists in core/src/store/mod.rs with open_or_init(path) and with_conn(closure) methods per spec signature
- [ ] #2 StoreError enum has Io / Sqlite / Migration variants with From impls for the underlying error types
- [ ] #3 Throwaway test runs Store::open_or_init against tempfile::tempdir and confirms PRAGMA user_version returns 0 (no migrations defined yet)
- [ ] #4 openwhisper-core crate compiles cleanly with rusqlite (bundled feature) + rusqlite_migration resolved as deps; license confirmed MIT/Apache-2.0 at land time
<!-- AC:END -->
