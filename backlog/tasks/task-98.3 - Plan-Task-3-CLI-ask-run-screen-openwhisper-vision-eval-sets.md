---
id: TASK-98.3
title: 'Plan Task 3: CLI ask run --screen, openwhisper vision, eval sets'
status: In Review
assignee: []
created_date: '2026-11-10T23:24:00Z'
updated_date: '2026-11-11T06:37:00Z'
labels:
  - 98-impl
dependencies: []
parent_task_id: TASK-98
ordinal: 108000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 ask run --screen sends a capture; vision check prints the trigger decision; vision probe prints sizes and timings and never image data
- [x] #2 Eval sets for the smart decision, reading synthetic images and on-screen text that gives instructions; offline self-tests pass
- [x] #3 Live eval runs are in the task notes
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
b4a59aa CLI and eval sets. CLI tests 58 passed, 2 ignored (live). Live evals, Claude CLI, Sonnet, low effort, 110 calls: smart decision 20/20 twice (10 needed and asked, 10 plain and answered, no misses either way); reading synthetic images 4/4 on every prompt; on-screen instructions 8/8 with the Phrases prompt twice, 7/8 and 8/8 with the Always prompt (the miss was a check that was too strict, loosened); Phrases text-only questions that need the screen 4/4 twice (says it cannot see it). Latency cold: text first text median 1.16 s, whole answer 1.66 s; image first text 1.21 s, whole answer 2.55 s. Not run: ask run --screen live (it would send the real screen).
<!-- SECTION:NOTES:END -->
