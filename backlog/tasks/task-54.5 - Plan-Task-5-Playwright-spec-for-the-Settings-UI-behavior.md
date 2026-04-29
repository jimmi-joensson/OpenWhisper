---
id: TASK-54.5
title: 'Plan Task 5: Playwright spec for the Settings UI behavior'
status: To Do
assignee: []
created_date: '2026-04-29 17:44'
labels:
  - 54-impl
dependencies: []
parent_task_id: TASK-54
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Tauri shim exposes autostart_get / autostart_set / autostart_supported stubs and emitAutostartChanged helper
- [ ] #2 Four new tests assert: initial state from autostart_get, write-through via autostart_set, external update via autostart_changed, dev-build disabled state with hint
- [ ] #3 Existing Settings + General-pane tests still pass
- [ ] #4 pnpm test:ui green locally and on CI
<!-- AC:END -->
