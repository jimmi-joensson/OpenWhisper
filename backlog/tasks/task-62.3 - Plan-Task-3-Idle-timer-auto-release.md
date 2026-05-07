---
id: TASK-62.3
title: 'Plan Task 3: Idle timer + auto-release'
status: In Review
assignee: []
created_date: '2026-04-30 22:25'
updated_date: '2026-05-07 00:00'
labels:
  - 62-impl
dependencies: []
parent_task_id: TASK-62
ordinal: 6000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 with_idle_timeout constructor and set_idle_timeout setter exist on ModelHandle
- [x] #2 After idle expires, handle transitions Loaded->Unloaded automatically
- [x] #3 Active calls cancel the timer; it re-arms on return to Loaded
- [x] #4 Duration::MAX (the keep-warm path) keeps the model resident indefinitely
- [x] #5 Async-runtime stance documented in module docs (no Tokio; std::thread + Condvar fallback per plan permission)
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
- Commit: f8e0a20.
- New `ModelHandle::with_idle_timeout(label, loader, idle)` ctor + `set_idle_timeout(new)` setter. `Duration::MAX` = keep warm; flipping to MAX cancels any pending fire immediately.
- **No Tokio in core/.** Plan permitted std::thread + Condvar fallback for the recognizer's minutes-cadence timer; chose that path to avoid pulling Tokio into core/ until TASK-63's cleanup-LLM async pipeline actually needs it. Public surface (`with_idle_timeout` / `set_idle_timeout`) is runtime-agnostic — future Tokio swap is non-breaking.
- One std::thread::spawn worker per handle, parked on `Condvar::wait_timeout`. Rearm = bump deadline + notify. Cancel = clear deadline + notify. `ShutdownSignal` Drop guard (held only on user-facing handle clones, not on the timer's captures) signals + joins when the last clone drops — detectable because timer thread doesn't hold the shutdown Arc. Test `dropping_last_clone_shuts_down_timer_thread` is the canary.
- Idle timer re-arms on every Loaded transition (post-load, post-use_with). use_with cancels the timer for the Active window and re-arms on return.
- Race-tolerant fire: timer may wake at deadline while use_with has just transitioned to Active → fire_unload sees state≠Loaded and skips. If timer wins the race, the next use_with auto-loads.
- Tests: 12 total in `model_lifecycle::tests`. 5 new for the timer (`idle_timer_unloads_after_deadline`, `use_with_extends_idle_window`, `keep_warm_via_duration_max_keeps_loaded`, `set_idle_timeout_to_max_cancels_pending_fire`, `dropping_last_clone_shuts_down_timer_thread`, `set_idle_timeout_errors_on_handle_without_timer`). `wait_until` poll helper avoids "sleep then check" flakiness. `cargo test -p openwhisper-core --lib` 76/76 green.
- Headless-first: ModelHandle still has no CLI surface — gating on TASK-62.4 (registry) per the openwhisper-headless-first skill's deferral clause.
- Awaiting user QA — pure-Rust, no UI surface yet.
<!-- SECTION:NOTES:END -->
