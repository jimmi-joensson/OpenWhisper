---
id: TASK-93.2
title: 'Plan Task 2: Claude CLI backend with warm process'
status: In Review
assignee: []
created_date: '2026-11-10T09:14:00Z'
updated_date: '2026-11-10T09:48:00Z'
labels:
  - 93-impl
dependencies: []
parent_task_id: TASK-93
ordinal: 66000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Binary found in usual install locations, PATH or settings override; not-installed and not-signed-in reported as distinct states
- [x] #2 Warm process spawned at record start answers a later request; a discarded one is killed
- [x] #3 Cold call works when no warm process exists
- [x] #4 Timeout kills the child and returns a timeout error
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
claude_cli backend. child PATH + Windows test module.
Verified on macOS against a fake CLI (unit tests) and the real CLI 2.1.322 (openwhisper refine test/run/eval), including a bare Finder-like environment.
Windows: cargo check --tests passes for x86_64-pc-windows-gnu; windows_tests (fake claude.cmd) are written but have NOT been run on a Windows machine yet.
<!-- SECTION:NOTES:END -->
