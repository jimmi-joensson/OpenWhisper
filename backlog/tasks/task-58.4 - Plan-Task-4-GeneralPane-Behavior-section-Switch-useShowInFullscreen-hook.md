---
id: TASK-58.4
title: 'Plan Task 4: GeneralPane Behavior section + Switch + useShowInFullscreen hook'
status: Done
assignee:
  - '@claude'
created_date: '2026-04-29 18:05'
updated_date: '2026-04-29 18:54'
labels:
  - 58-impl
dependencies: []
parent_task_id: TASK-58
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 New use-show-in-fullscreen.ts hook uses invoke + listen, matching the project's Settings hook pattern
- [x] #2 General pane has a Switch row in a Behavior section (or merged into an existing section) with the spec's description copy
- [x] #3 Toggling the Switch persists, updates the cache, and is reflected back via the listen subscription
- [x] #4 pnpm tsc --noEmit clean
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
1d90c96 new useShowInFullscreen hook + Behavior section (between Appearance and Updates) with Switch row; tsc --noEmit clean
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added useShowInFullscreen hook (invoke + listen) and a new General → Behavior section with a Switch row. tsc --noEmit clean.
<!-- SECTION:FINAL_SUMMARY:END -->
