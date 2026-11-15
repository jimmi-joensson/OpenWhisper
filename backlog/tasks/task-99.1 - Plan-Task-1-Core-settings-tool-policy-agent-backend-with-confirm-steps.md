---
id: TASK-99.1
title: 'Plan Task 1: Core settings, tool policy, agent backend with confirm, steps'
status: In Review
assignee: []
created_date: '2026-11-11T06:38:00Z'
updated_date: '2026-11-11T11:48:00Z'
labels:
  - 99-impl
dependencies: []
parent_task_id: TASK-99
ordinal: 111000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Tool classification tested on a table of tool names and annotations; unknown is act
- [ ] #2 Spawn arguments pinned by a test: permission mode default, empty setting sources, strict MCP config, no built-in tools, no session persistence
- [ ] #3 Fake CLI: allowed tool calls become steps; a can_use_tool request for a ticked act tool becomes a confirm that allow and deny answer; an unexpected request is denied without a confirm; a confirm that times out is a deny; cancel kills the process
- [ ] #4 Drafts only: act tools are disallowed whatever the allow list says
- [ ] #5 Generated config holds only enabled connections, is private and removed afterwards; connection details never reach settings.json (test)
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Built in d394b56. Settings block, tool policy (read, draft, act; in doubt act), agent backend with pinned arguments, confirm over the permission prompt, steps and context source on the ask snapshot, connection before screen routing, tools=N in history. Orchestrator's own run 2026-11-11: core lib 844 passed (763 before), Windows target checks clean. Deviations: details of the person's own servers (environment, headers) are read from the Claude config into memory only, because the CLI listing leaves them out; the listing runs with default setting sources and hooks disabled, because with no setting sources the user-level servers do not show. A tool whose name has both send and draft counts as act.
<!-- SECTION:NOTES:END -->
