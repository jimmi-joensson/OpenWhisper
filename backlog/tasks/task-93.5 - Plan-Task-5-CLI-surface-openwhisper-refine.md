---
id: TASK-93.5
title: 'Plan Task 5: CLI surface - openwhisper refine'
status: In Review
assignee: []
created_date: '2026-11-10T09:14:00Z'
updated_date: '2026-11-10T09:48:00Z'
labels:
  - 93-impl
dependencies: []
parent_task_id: TASK-93
ordinal: 69000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 refine run refines an argument or stdin and prints text or a JSON outcome
- [x] #2 refine status reports settings and backend availability
- [x] #3 refine test runs the same connection test as the UI
- [x] #4 refine config reads and writes the settings block; set-key and clear-key manage the keychain entry
- [x] #5 refine eval runs the live eval set with pass/fail and latency per case
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
openwhisper refine run|status|test|config|set-key|clear-key|eval. All run by hand against the real Claude CLI on macOS.
Note: the CLI reads com.openwhisper.app; set OPENWHISPER_CONFIG_DIR to point it at the dev build (com.openwhisper.app.dev).
<!-- SECTION:NOTES:END -->
