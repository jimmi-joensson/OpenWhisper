---
id: TASK-81.8
title: 'Plan Task 8: cli crash-dump (stub for TASK-78)'
status: In Review
assignee: []
created_date: '2026-05-04 15:10'
updated_date: '2026-05-06'
labels:
  - 81-impl
milestone: m-1
dependencies: []
parent_task_id: TASK-81
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Subcommand surface exists but stubbed until TASK-78's CrashDump reader lands. Flags: --latest, --id <n>, --list. Prints deferred-feature notice to stderr; exits 0 so CI smoke does not break.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 cli crash-dump --help lists --latest / --id / --list flags
- [x] #2 Each flag invocation prints deferred-feature notice and exits 0
- [x] #3 Code includes TODO comment referencing TASK-78 explicitly
- [x] #4 Handler imports core::diagnostics::CrashDumpReader (or default_crash_reader() accessor) — proving contract is real, not informal
- [x] #5 Once TASK-78 ships its concrete reader, the CLI handler swaps without redesign (default_crash_reader() returns Some)
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Landed in commit `c14f61e`. Handler dispatches to `core::diagnostics::default_crash_reader()`; today returns None and the CLI prints "crash reporting not yet enabled — see TASK-78" on stderr (exit 0). --json mode also writes `{"available": false}` to stdout so pipelines stay valid. Per-mode dispatch (`list` / `show_latest` / `show_by_id`) is wired against the `&dyn CrashDumpReader` shape so TASK-78 swaps in a `Some(FileBackedCrashDumpReader)` without redesign.
<!-- SECTION:NOTES:END -->
