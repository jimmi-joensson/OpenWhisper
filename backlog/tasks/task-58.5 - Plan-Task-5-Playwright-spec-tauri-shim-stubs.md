---
id: TASK-58.5
title: 'Plan Task 5: Playwright spec + tauri shim stubs'
status: To Do
assignee: []
created_date: '2026-04-29 18:05'
labels:
  - 58-impl
dependencies: []
parent_task_id: TASK-58
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Tauri shim exposes stubs for the two behavior commands plus an emitShowInFullscreenChanged helper
- [ ] #2 Three new tests assert: initial state from behavior_get, write-through via behavior_set, external event update via behavior_show_in_fullscreen_changed
- [ ] #3 Existing Settings + General-pane tests still pass
- [ ] #4 pnpm test:ui green locally and on CI
<!-- AC:END -->
