---
id: TASK-93.1
title: 'Plan Task 1: Core refine module - trait, prompt, orchestration, settings'
status: In Review
assignee: []
created_date: '2026-11-10T09:14:00Z'
updated_date: '2026-11-10T09:48:00Z'
labels:
  - 93-impl
dependencies: []
parent_task_id: TASK-93
ordinal: 65000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Refiner trait and RefineOutcome are public and re-exported from the prelude
- [x] #2 Composed prompt holds guard rules, user or default instructions and dictionary terms; transcript only in the user message
- [x] #3 Orchestrator returns unrefined text on timeout, error, unavailable backend and guard rejection, each with a distinct status
- [x] #4 Utterances under the minimum word count never reach a backend
- [x] #5 refine block round-trips through settings.json without clobbering sibling blocks; default disabled
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
core::refine (mod, prompt, guard), settings refine block, dictionary terms, transcript::finalize_refined.
prompt rules from live runs.
Verification: cargo test -p openwhisper-core --no-default-features --features tauri --lib -> 216 passed.
<!-- SECTION:NOTES:END -->
