---
id: TASK-62.4
title: 'Plan Task 4: Performance settings (keep_models_warm)'
status: Done
assignee: []
created_date: '2026-04-30 22:25'
updated_date: '2026-05-07 22:21'
labels:
  - 62-impl
dependencies: []
parent_task_id: TASK-62
ordinal: 7000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 PerformanceSettings schema added; keep_models_warm defaults false
- [x] #2 settings_set_keep_models_warm(true) persists JSON, flips atomic, and reconfigures every registered ModelHandle's timer in the same call
- [x] #3 Registered handles correctly update on flip without app restart
- [x] #4 Both Tauri commands wired in invoke_handler!
- [x] #5 cargo check clean
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
- Commit: 50e4a30.
- New `core::settings::PerformanceSettings { keep_models_warm: bool }` (Default: false). `SettingsFile.performance` field added; all 5 sibling save_* functions thread it through to avoid clobbering on parallel saves. `keep_models_warm()` lock-free reader + `set_keep_models_warm_cache(value)` setter + `current_/load_/save_performance_settings`.
- `core::model_lifecycle::apply_keep_warm(value)` walks a process-global `Vec<Weak<IdleControl>>` registry and flips each handle's `keep_warm` AtomicBool. Dead Weaks are pruned on every sweep (via `retain`). Registry-detection of dropped handles uses `Weak::ptr_eq` so the test stays correct under parallel execution.
- Refactored `IdleControl` to separate `configured_timeout` (user's setting; `set_idle_timeout` writes it) from `keep_warm` (cluster override; `apply_keep_warm` writes it). `effective_timeout()` combines them — flipping keep-warm OFF restores the original user timeout instead of MAX-stickiness.
- New ModelHandle constructions inherit current `KEEP_MODELS_WARM` via `crate::settings::keep_models_warm()` so handles spun up after a flip see the right value.
- Tauri: `settings_get_performance` + `settings_set_keep_models_warm` (writes JSON → flips atomic inside core's save → calls `apply_keep_warm`). Both registered in `invoke_handler!`. Boot-time hydrate calls `apply_keep_warm(perf.keep_models_warm)` so the lock-free atomic is set before any handle is constructed.
- **Headless-first parity** — `cli settings get-performance` + `cli settings set-keep-models-warm <true|false>`. Resolves the same `~/Library/Application Support/com.openwhisper.app/settings.json` Tauri uses (via `dirs` crate + bundle id). Smoked locally; reads the GUI's persisted setting. Per the openwhisper-headless-first skill rule.
- Prelude re-exports: `PerformanceSettings`, `keep_models_warm`, `apply_keep_warm`.
- 4 new tests in `model_lifecycle::tests` (`apply_keep_warm_true_cancels_pending_fire`, `apply_keep_warm_false_rearms_with_configured_timeout`, `dropped_handles_fall_out_of_registry`, plus existing `keep_warm_via_duration_max_keeps_loaded`/`set_idle_timeout_to_max_cancels_pending_fire`). 3 new in `settings::tests` (`performance_default_keep_models_warm_is_false`, `performance_legacy_json_without_block_defaults_false`, `performance_save_load_round_trip_via_disk` using tempfile). cargo test -p openwhisper-core --lib 82/82 green; tauri + cli check clean.
- Awaiting user QA — the persistence + atomic + apply_keep_warm path is reachable via `cli settings set-keep-models-warm true` followed by relaunch of the GUI.
<!-- SECTION:NOTES:END -->
