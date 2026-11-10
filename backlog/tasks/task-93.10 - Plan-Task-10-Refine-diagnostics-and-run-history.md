---
id: TASK-93.10
title: 'Plan Task 10: Refine diagnostics and run history'
status: In Review
assignee: []
created_date: '2026-11-10T10:57:00Z'
labels:
  - 93-impl
dependencies: []
parent_task_id: TASK-93
ordinal: 74000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Each Refine run is stored with status, timing, warm or cold start, model, effort and reason, and no dictated text
- [ ] #2 Diagnostics shows connection state, response times, outcomes and recent runs, with test and clear
- [ ] #3 openwhisper refine history reads the same data; openwhisper refine prompt prints the exact prompt
- [ ] #4 Verbose log shows how long a hotkey press takes to become a live mic, separate from the Refine warm-up
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Migration 3 (refine_runs). 241 core tests, 158 Playwright. The mic-live timing is in the build but has not been read from a real run yet: the Dev app that was open during this work predates it.
<!-- SECTION:NOTES:END -->
