---
id: TASK-61.6
title: 'Plan Task 6: Playwright spec + tauri shim stubs'
status: To Do
assignee: []
created_date: '2026-04-30 22:18'
labels:
  - 61-impl
dependencies: []
parent_task_id: TASK-61
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Tauri shim exposes stubs for both pause-audio commands plus emitPauseAudioChanged helper
- [ ] #2 Test asserts Switch reflects behavior_get value on mount
- [ ] #3 Test asserts toggling Switch invokes behavior_set with the new value
- [ ] #4 Test asserts behavior_pause_audio_changed event updates the Switch
- [ ] #5 Existing settings + general-pane + show-in-fullscreen tests still pass
- [ ] #6 pnpm test:ui green locally
<!-- AC:END -->
