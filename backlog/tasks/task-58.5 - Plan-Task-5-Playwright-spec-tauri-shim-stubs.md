---
id: TASK-58.5
title: 'Plan Task 5: Playwright spec + tauri shim stubs'
status: Done
assignee:
  - '@claude'
created_date: '2026-04-29 18:05'
updated_date: '2026-04-29 18:55'
labels:
  - 58-impl
dependencies: []
parent_task_id: TASK-58
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Tauri shim exposes stubs for the two behavior commands plus an emitShowInFullscreenChanged helper
- [x] #2 Three new tests assert: initial state from behavior_get, write-through via behavior_set, external event update via behavior_show_in_fullscreen_changed
- [x] #3 Existing Settings + General-pane tests still pass
- [x] #4 pnpm test:ui green locally and on CI
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
c7d50e6 shim stubs + emitShowInFullscreenChanged helper + 3 new tests; pnpm test:ui 43/43 green
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added shim stubs for behavior_get/set_show_in_fullscreen, emitShowInFullscreenChanged helper, and three Playwright tests covering mount-state, click-through, and external-event update. pnpm test:ui 43/43 green.
<!-- SECTION:FINAL_SUMMARY:END -->
