---
id: TASK-65.9
title: 'Plan Task 9: Reset settings pane on route exit'
status: Done
assignee: []
created_date: '2026-05-01 14:26'
updated_date: '2026-05-01 14:50'
labels: []
dependencies: []
parent_task_id: TASK-65
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
When leaving the Settings route, reset settingsPane to 'general'. Re-entering Settings always lands on General — keeps in-Settings nav lossless without making the route exit feel like a partial back.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Re-entering Settings always lands on General, even if a different pane was active when leaving.
- [ ] #2 Pane state is preserved while inside Settings (clicking around between panes does not reset).
- [ ] #3 Playwright covers the reset.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented in same commit as the test. Awaiting user QA in pnpm dev:tauri.
<!-- SECTION:NOTES:END -->
