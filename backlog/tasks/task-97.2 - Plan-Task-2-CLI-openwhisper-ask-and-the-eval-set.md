---
id: TASK-97.2
title: 'Plan Task 2: CLI openwhisper ask and the eval set'
status: In Review
assignee: []
created_date: '2026-11-10T17:37:00Z'
updated_date: '2026-11-10T22:20:00Z'
labels:
  - 97-impl
dependencies: []
parent_task_id: TASK-97
ordinal: 101000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 ask run streams the answer and prints first-text and total time; --json gives one record; overrides do not save
- [x] #2 ask status, ask config and ask eval exist
- [x] #3 Eval set passes its offline self-test
- [x] #4 Live eval run and latency per backend are in the task notes
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
1f6799d CLI. 19 unit + 9 ask + 9 modes + 1 smoke tests. Live, Claude CLI 2.1.322, Sonnet, low effort, scratch config: cold first text 1.3 to 1.8 s, whole answer 1.8 to 3.5 s; warmed 3 s ahead first text 0.6 to 1.2 s, whole answer 1.2 to 1.6 s; with web search and a search needed 11 s (text in one burst), no search needed 1.5 s. ask eval 12/12 twice; first text median 1.1 to 1.2 s, whole answer median 1.7 to 1.8 s, max 3.7 s. Orchestrator re-run of one ask: first text 1.1 s, done 2.0 s, 1392 tokens in (864 cached), 95 out. OpenAI-compatible backend: not measured, no endpoint.
<!-- SECTION:NOTES:END -->
