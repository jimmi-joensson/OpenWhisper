---
id: TASK-78.1
title: 'Plan Task 1: Crash file schema + Rust panic hook'
status: In Review
assignee:
  - '@claude'
created_date: '2026-05-04 06:16'
updated_date: '2026-05-07 16:44'
labels:
  - 78-impl
milestone: m-1
dependencies: []
parent_task_id: TASK-78
ordinal: 35000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Redactor strips home-dir paths and env-token patterns from all string fields including backtrace
- [x] #2 Recording-state snapshot uses try_lock and degrades to null if state lock is held by panicker
- [x] #3 Unit tests for redaction + serde round-trip committed and green
- [x] #4 Default Rust panic stderr output still prints after the hook (chained, not replaced)
- [x] #5 A panic on any thread produces a crash file at <app_log_dir>/crashes/<unix-ms>.json conforming to schema v1 (rust_panic, recording_state, events)
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
33236d0 crash file schema + Rust panic hook (chained, redacted, with event buffer)
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Crash schema + panic hook landed in 33236d0. core::crashes module owns the typed CrashFile schema (schema_version=1, rust_panic, recording_state, events) plus a per-field redactor (Unix/Win user paths in 4 variants, runtime $HOME, env-token assignments). Panic hook installs once at Tauri setup, captures from any thread, drains the 64-entry event buffer, takes a try_lock snapshot of dictation state (None on contention or PHASE_IDLE), redacts every String field, writes <app_log_dir>/crashes/<unix-ms>.json, and chains the previous hook so default stderr is preserved. 16 unit tests + 1 integration test (panic_on_thread_writes_chained_crash_file) green; full core suite 104/104.
<!-- SECTION:FINAL_SUMMARY:END -->
