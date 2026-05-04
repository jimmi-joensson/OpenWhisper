---
id: TASK-78.1
title: 'Plan Task 1: Crash file schema + Rust panic hook'
status: To Do
assignee: []
created_date: '2026-05-04 06:16'
updated_date: '2026-05-04 08:03'
labels:
  - 78-impl
dependencies: []
parent_task_id: TASK-78
milestone: m-1
ordinal: 35000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Redactor strips home-dir paths and env-token patterns from all string fields including backtrace
- [ ] #2 Recording-state snapshot uses try_lock and degrades to null if state lock is held by panicker
- [ ] #3 Unit tests for redaction + serde round-trip committed and green
- [ ] #4 Default Rust panic stderr output still prints after the hook (chained, not replaced)
- [ ] #5 A panic on any thread produces a crash file at <app_log_dir>/crashes/<unix-ms>.json conforming to schema v1 (rust_panic, recording_state, events)
<!-- AC:END -->
