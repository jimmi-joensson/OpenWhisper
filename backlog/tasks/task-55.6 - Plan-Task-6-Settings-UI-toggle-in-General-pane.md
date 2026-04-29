---
id: TASK-55.6
title: 'Plan Task 6: Settings UI toggle in General pane'
status: To Do
assignee: []
created_date: '2026-04-29 08:01'
updated_date: '2026-04-29 08:27'
labels:
  - 55-impl
dependencies:
  - TASK-56
parent_task_id: TASK-55
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Surface 'Follow active screen' in Settings → General, default ON, persisting via the commands from Plan Task 1. Visual treatment matches the existing Audio pane mic-test toggle.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 General pane renders the toggle and reflects persisted state on open
- [ ] #2 Flipping the toggle updates settings.json AND the in-process atomic on the same interaction (no restart needed)
- [ ] #3 New users see toggle in the ON position by default
- [ ] #4 Uses shadcn Switch + Field primitives consistent with the rest of GeneralPane (TASK-56)
<!-- AC:END -->
