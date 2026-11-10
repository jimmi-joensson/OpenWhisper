---
id: TASK-97.5
title: 'Plan Task 5: UI answer panel, Ask pane, asks in history'
status: In Review
assignee: []
created_date: '2026-11-10T17:37:00Z'
updated_date: '2026-11-10T23:23:00Z'
labels:
  - 97-impl
dependencies: []
parent_task_id: TASK-97
ordinal: 104000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Panel renders every phase from ask_tick and its buttons call the matching commands
- [x] #2 Ask pane switches Ask and web search and shows where questions are sent
- [x] #3 An ask in history is marked and shows its answer
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
f77c77d answer panel (card 396 px wide, up to 336 px high, anchored to the bottom of the 420 x 360 window; one 180 ms fade on appear; follows the main window's theme), Ask pane, ask rows in history and diagnostics. Hooks on TanStack Query (lib/use-ask.ts). Playwright: answer-window.spec.ts 26, ask.spec.ts 24. Looks not judged by a person; clicks and text selection in a window that never takes focus not verified.
<!-- SECTION:NOTES:END -->
