---
id: TASK-62.2
title: 'Plan Task 2: ModelHandle<T> state machine (no timer)'
status: To Do
assignee: []
created_date: '2026-04-30 22:25'
labels:
  - 62-impl
dependencies: []
parent_task_id: TASK-62
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 LifecycleState and ModelHandle<T> defined in core/src/model_lifecycle/mod.rs
- [ ] #2 load(), unload(), use_with(), state(), current_memory_estimate() public on the handle
- [ ] #3 current_memory_estimate() returns RSS delta captured at most recent Loading->Loaded transition
- [ ] #4 Unit tests cover load idempotency, auto-load on use, unload-while-active rejection, failed-loader cleanup
- [ ] #5 cargo check and cargo test clean
<!-- AC:END -->
