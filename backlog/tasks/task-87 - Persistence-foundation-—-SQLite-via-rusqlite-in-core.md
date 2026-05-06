---
id: TASK-87
title: Persistence foundation — SQLite via rusqlite in core/
status: To Do
assignee: []
created_date: '2026-05-06 06:06'
updated_date: '2026-05-06 06:10'
labels: []
dependencies: []
documentation:
  - backlog/docs/specs/doc-39 - Persistence-foundation-—-design.md
  - backlog/docs/plans/doc-40 - Persistence-foundation-—-implementation-plan.md
ordinal: 47000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add a thin persistence layer in the core/ Rust crate using rusqlite (bundled SQLite) + rusqlite_migration for schema versioning. Single DB file at app_data_dir/openwhisper.db (path passed in from Tauri shell so core stays platform-agnostic). Schema migration 1 creates the dictations table that the stats feature (TASK-88) and the future history feature will write to. No user-visible surface; pure infra. Identifier com.openwhisper.app stays stable across the TASK-85 rename so existing data is not orphaned.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 core/Cargo.toml depends on rusqlite (bundled feature) + rusqlite_migration; both are MIT-compatible
- [ ] #2 core/src/store/mod.rs exposes open_or_init(path: &Path) -> Result<Connection, StoreError> that creates the file if missing and runs all pending migrations idempotently
- [ ] #3 Migration 1 creates dictations table with columns: id INTEGER PK AUTOINCREMENT, started_at INTEGER NOT NULL, duration_ms INTEGER NOT NULL, word_count INTEGER NOT NULL, transcript TEXT NULL, confidence REAL NULL, app_bundle_id TEXT NULL, created_at INTEGER NOT NULL DEFAULT — plus index on started_at
- [ ] #4 Tauri shell startup calls open_or_init with app.path().app_data_dir() and stores the connection in managed state; init failure surfaces via dictation_deliver_error rather than panicking
- [ ] #5 Cross-platform smoke: file lands at ~/Library/Application Support/com.openwhisper.app/openwhisper.db on Mac and %APPDATA%\\com.openwhisper.app\\openwhisper.db on Win
- [ ] #6 Unit tests cover open-then-reopen round-trip, migration idempotency, and concurrent-read safety
<!-- AC:END -->
