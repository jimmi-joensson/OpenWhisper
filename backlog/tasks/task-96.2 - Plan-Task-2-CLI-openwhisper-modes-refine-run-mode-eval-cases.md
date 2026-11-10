---
id: TASK-96.2
title: 'Plan Task 2: CLI openwhisper modes, refine run --mode, eval cases'
status: In Review
assignee: []
created_date: '2026-11-10T16:13:00Z'
updated_date: '2026-11-10T16:48:00Z'
labels:
  - 96-impl
dependencies: []
parent_task_id: TASK-96
ordinal: 95000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Every modes operation is reachable from the CLI with --json
- [x] #2 refine run --mode uses the same core function as the pipeline
- [x] #3 Eval set has at least three custom-mode cases and its offline self-test passes
- [x] #4 Live eval run recorded in the task notes
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
ccb4bf3 CLI. 12 unit + 5 integration tests pass. Live eval (Claude CLI, Sonnet, low effort, cold, scratch config): 23/24, 24/24, 24/24; custom-mode cases 4/5, 5/5, 5/5. The one miss was the eval's Formal mode keeping a casual greeting; its instructions were reworded once, then 15/15 over three runs of the mode cases. Latency median/max: built-in 1.5 to 1.6 s / 3.1 to 3.8 s, custom modes 1.6 to 1.8 s / 1.7 to 2.9 s. The injection case came back as the sentence itself every time.
<!-- SECTION:NOTES:END -->
