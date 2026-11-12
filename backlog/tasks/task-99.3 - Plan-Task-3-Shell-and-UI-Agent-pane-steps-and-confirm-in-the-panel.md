---
id: TASK-99.3
title: 'Plan Task 3: Shell and UI Agent pane, steps and confirm in the panel'
status: In Review
assignee: []
created_date: '2026-11-11T06:38:00Z'
updated_date: '2026-11-12T14:00:00Z'
labels:
  - 99-impl
dependencies: []
parent_task_id: TASK-99
ordinal: 113000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Agent can be switched on with its plain statement; connections can be refreshed, enabled and their tools ticked
- [ ] #2 Panel shows each step as it happens and a confirm with the full input; Allow and Deny answer it
- [ ] #3 History marks asks that used tools and lists them
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Built in 0ac48d3. Shell commands agent_get, agent_set (choices only), agent_refresh, agent_status, agent_add_connection, agent_remove_connection, agent_running_apps, ask_confirm_allow, ask_confirm_deny. Settings > Agent pane, steps and confirm card in the answer panel, Confirm label on the pill, tools on history rows, both shims. Orchestrator's own run 2026-11-12: shell cargo 87+1 passed, Windows target check clean, tsc clean, vitest 40, Playwright 414 of 414 (365 before). Not verifiable here: a real refresh at start-up, clicks on a real confirm card in the non-focus panel, the look of pane and panel.
<!-- SECTION:NOTES:END -->
