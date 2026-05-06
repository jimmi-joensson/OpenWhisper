---
id: TASK-87.1
title: 'Plan Task 1: rusqlite + rusqlite_migration deps + Store module skeleton'
status: Done
assignee:
  - '@claude'
created_date: '2026-05-06 06:09'
updated_date: '2026-05-06 19:02'
labels:
  - 87-impl
dependencies: []
parent_task_id: TASK-87
ordinal: 48000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Store struct exists in core/src/store/mod.rs with open_or_init(path) and with_conn(closure) methods per spec signature
- [x] #2 StoreError enum has Io / Sqlite / Migration variants with From impls for the underlying error types
- [x] #3 Throwaway test runs Store::open_or_init against tempfile::tempdir and confirms PRAGMA user_version returns 0 (no migrations defined yet)
- [x] #4 openwhisper-core crate compiles cleanly with rusqlite (bundled feature) + rusqlite_migration resolved as deps; license confirmed MIT/Apache-2.0 at land time
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Commit: 4c4e54d on branch task-87-persistence (worktree at ../OpenWhisper-task87). cargo test -p openwhisper-core --lib green: 27 tests pass (25 prior + 2 new store::tests). cargo build -p openwhisper-core clean. No user-visible surface — pure infra; awaiting code review. Pre-existing example 'recognizer_smoke' fails to compile without --features recognizer, unchanged by this task.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Merged via PR #16 (squash 2415f3a). Cross-platform smoke green: Mac + Windows.
<!-- SECTION:FINAL_SUMMARY:END -->
