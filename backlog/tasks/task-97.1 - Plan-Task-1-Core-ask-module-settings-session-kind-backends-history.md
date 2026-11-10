---
id: TASK-97.1
title: 'Plan Task 1: Core ask module, settings, session kind, backends, history'
status: To Do
assignee: []
created_date: '2026-11-10T17:37:00Z'
labels:
  - 97-impl
dependencies: []
parent_task_id: TASK-97
ordinal: 100000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 AskSettings defaults are off with no web search; cleaned clamps the timeout; old settings files load
- [ ] #2 A recording started as an ask never reaches Refine, the injector or the stats writer; dictation is unchanged (tests)
- [ ] #3 Claude backend against a fake CLI: streams text in order, reports first-text time and usage, is killed on cancel and timeout, always passes --permission-mode default; web search adds exactly the WebSearch flags
- [ ] #4 OpenAI-compatible backend against a fake SSE server: streams, stops on DONE, reports HTTP errors plainly
- [ ] #5 A finished ask appends a history entry with kind=ask and the answer; old files parse as before and plain entries render byte for byte as before
<!-- AC:END -->
