---
id: TASK-61.2
title: 'Plan Task 2: MediaController trait + phase observer'
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
- [ ] #1 media_control module exists with MediaController trait + cfg-gated PlatformMediaController
- [ ] #2 Stub Mac and Windows impls compile with pause_now() returning false and resume_now() no-op
- [ ] #3 spawn_dictation_emitter detects RECORDING entry/exit and calls trait methods only when cache=on and only when entry actually paused something
- [ ] #4 Observer state stays local to the emitter thread; no new threads or shared state
- [ ] #5 Manual smoke with stubs shows no behavior change and no panic
<!-- AC:END -->
