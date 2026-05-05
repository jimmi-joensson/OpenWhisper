---
id: TASK-81.8
title: 'Plan Task 8: cli crash-dump (stub for TASK-78)'
status: To Do
assignee: []
created_date: '2026-05-04 15:10'
updated_date: '2026-05-04 15:17'
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
- [ ] #1 cli crash-dump --help lists --latest / --id / --list flags
- [ ] #2 Each flag invocation prints deferred-feature notice and exits 0
- [ ] #3 Code includes TODO comment referencing TASK-78 explicitly
- [ ] #4 Handler imports core::diagnostics::CrashDumpReader (or default_crash_reader() accessor) — proving contract is real, not informal
- [ ] #5 Once TASK-78 ships its concrete reader, the CLI handler swaps without redesign (default_crash_reader() returns Some)
<!-- AC:END -->
