---
id: TASK-55.6
title: 'Plan Task 6: Settings UI toggle in General pane'
status: Done
assignee:
  - '@claude'
created_date: '2026-04-29 08:01'
updated_date: '2026-04-29 19:04'
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
- [x] #1 General pane renders the toggle and reflects persisted state on open
- [x] #2 Flipping the toggle updates settings.json AND the in-process atomic on the same interaction (no restart needed)
- [x] #3 New users see toggle in the ON position by default
- [x] #4 Uses shadcn Switch + Field primitives consistent with the rest of GeneralPane (TASK-56)
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
8694f8b General pane Pill section: Switch + invoke wiring + optimistic update with revert.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added Pill section in GeneralPane between Appearance and Updates. Switch hydrated from settings_get_pill (defaults ON on rejection, matching Rust-side default). Toggle flip = optimistic UI update + invoke settings_set_pill_follow, with state revert + console.warn on rejection. Uses the same Field/Switch primitives as launch-at-login. tsc clean.
<!-- SECTION:FINAL_SUMMARY:END -->
