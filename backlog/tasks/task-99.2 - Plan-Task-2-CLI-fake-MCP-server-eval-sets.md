---
id: TASK-99.2
title: 'Plan Task 2: CLI, fake MCP server, eval sets'
status: To Do
assignee: []
created_date: '2026-11-11T06:38:00Z'
labels:
  - 99-impl
dependencies: []
parent_task_id: TASK-99
ordinal: 112000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 agent run against the fake server reads and drafts; with drafts only a send never reaches the fake server log; with it off, confirm deny blocks it and confirm allow lets it through
- [ ] #2 agent connections --refresh lists connections without a tool call
- [ ] #3 Eval sets with planted instructions in a tool result and in on-screen text, judged by the fake server log; live numbers and run times in the notes
<!-- AC:END -->
