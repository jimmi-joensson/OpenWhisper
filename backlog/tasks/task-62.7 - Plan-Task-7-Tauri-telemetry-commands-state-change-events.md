---
id: TASK-62.7
title: 'Plan Task 7: Tauri telemetry commands + state-change events'
status: In Review
assignee: []
created_date: '2026-04-30 22:25'
updated_date: '2026-05-07 00:00'
labels:
  - 62-impl
dependencies: []
parent_task_id: TASK-62
ordinal: 10000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 telemetry_get_memory Tauri command returns MemoryStats with per-model rows
- [x] #2 model-state-changed event fires on every Lifecycle transition with { label, state }
- [x] #3 Event includes both recognizer transitions and any future cleanup transitions
- [x] #4 cargo check clean
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
- New `core::telemetry::{ModelMemoryRow, MemoryStats, collect_memory_stats}`. Aggregates `query_process_memory` + `model_lifecycle::registry_snapshot()` into a single Serialize-friendly readout the Tauri command can ship to React unchanged.
- `core::model_lifecycle::registry_snapshot()` walks the existing Weak registry, reads each handle's label / state / RSS-delta, prunes dead Weaks as a side effect. Reads do NOT acquire the handle's `inner` mutex — telemetry must not contend with active transcription. Crucially, Tauri's 1 Hz poll never blocks on `use_with`.
- Refactor for telemetry surfacing: `IdleControl` extended to carry `label: String`, `state: Arc<Mutex<LifecycleState>>`, `last_load_rss_delta: Arc<Mutex<u64>>` (Arc clones from the user-facing handle). Avoided the bigger `Arc<HandleInner>` refactor.
- `core::model_lifecycle::on_state_change(StateChangeCallback)` callback registry; `notify_state_change(label, new)` helper called inside every transition site (load: Loading/Loaded/Unloaded-on-failure; unload: Releasing/Unloaded; use_with: Active/Loaded; fire_unload: Releasing/Unloaded). Callbacks fire AFTER the state lock is dropped so callbacks that re-enter `state()` see the new value.
- `LifecycleState` + `ProcessMemory` + `ModelMemoryRow` + `MemoryStats` all derive Serialize/Deserialize. `ProcessMemory.timestamp` (was `SystemTime`) replaced with `timestamp_unix_ms: u64` so it round-trips through serde without a custom serializer.
- Tauri: new `telemetry_get_memory` command + setup callback registers `on_state_change` that emits `app.emit("model-state-changed", { label, state })`. Both registered in `invoke_handler!`. Bridging happens through the existing `APP_HANDLE` `OnceLock`.
- **Headless-first parity** — `cli memory --models` flag. Calls `recognizer_ensure_loaded` to seed the registry, then prints per-model rows from `collect_memory_stats`. JSON path serializes `MemoryStats` directly.
- Prelude re-exports: `ModelMemoryRow`, `MemoryStats`, `collect_memory_stats`, `StateChangeCallback`, `on_state_change`, `registry_snapshot`.
- 2 new tests: `on_state_change_fires_for_every_transition_in_load_use_unload_cycle` (asserts the 6-event sequence Loading/Loaded/Active/Loaded/Releasing/Unloaded; filters by label so parallel tests don't bleed into the observation), `registry_snapshot_yields_label_and_state_per_live_handle` (two handles + load → snapshot has both with correct states).
- 84/84 lib tests green. Tauri + cli check clean.
- Awaiting user QA — `model-state-changed` flow can be smoked via `pnpm dev:tauri` and the (yet-unbuilt) Diagnostics → Memory pane (TASK-62.8); for now, headless path reachable via `cli memory --models`.
<!-- SECTION:NOTES:END -->
