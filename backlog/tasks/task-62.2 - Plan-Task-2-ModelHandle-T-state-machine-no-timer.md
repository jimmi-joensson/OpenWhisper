---
id: TASK-62.2
title: 'Plan Task 2: ModelHandle<T> state machine (no timer)'
status: Done
assignee: []
created_date: '2026-04-30 22:25'
updated_date: '2026-05-07 22:21'
labels:
  - 62-impl
dependencies: []
parent_task_id: TASK-62
ordinal: 5000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 LifecycleState and ModelHandle<T> defined in core/src/model_lifecycle/mod.rs
- [x] #2 load(), unload(), use_with(), state(), current_memory_estimate() public on the handle
- [x] #3 current_memory_estimate() returns RSS delta captured at most recent Loading->Loaded transition
- [x] #4 Unit tests cover load idempotency, auto-load on use, unload-while-active rejection, failed-loader cleanup
- [x] #5 cargo check and cargo test clean
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
- Commit: 14aefc9.
- New `core::model_lifecycle::{LifecycleState, ModelHandle<T>}`. `pub mod model_lifecycle;` wired in `core/src/lib.rs`.
- Synchronous state machine, no Tokio yet (idle timer lands TASK-62.3). Concurrency model: `state` and `inner` each `Arc<Mutex<…>>`; loader runs *between* lock acquisitions so a long load doesn't block the diagnostics 1 Hz `state()` poll. Single-flight `load()` errors a second concurrent caller with `ErrLoadInFlight` — proper condvar awaiting comes with the runtime in 62.3.
- `unload()` rejects from `Active`, `Loading`, `Releasing`; only legal from `Loaded`/`Unloaded`. Failed loader resets to `Unloaded` so callers can retry.
- 6 unit tests: load→use→unload sequence, idempotent load, auto-load via `use_with`, unload-while-Active rejected, failed loader propagates + leaves state Unloaded, current_memory_estimate sane on first load. `cargo test -p openwhisper-core --lib` 70/70 green. `cargo check -p openwhisper-core --features tauri` and `cargo check -p openwhisper-tauri` clean.
- Awaiting user QA — pure-Rust, no UI surface yet. Reviewer can verify via `cargo test -p openwhisper-core --lib model_lifecycle`.
<!-- SECTION:NOTES:END -->
