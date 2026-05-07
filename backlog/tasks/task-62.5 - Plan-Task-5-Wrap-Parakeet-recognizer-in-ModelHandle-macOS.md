---
id: TASK-62.5
title: 'Plan Task 5: Wrap Parakeet recognizer in ModelHandle (macOS)'
status: Done
assignee: []
created_date: '2026-04-30 22:25'
updated_date: '2026-05-07 22:21'
labels:
  - 62-impl
dependencies: []
parent_task_id: TASK-62
ordinal: 8000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 ENGINE is a ModelHandle<Box<dyn Recognizer>> with 5-min idle timeout
- [x] #2 recognizer_ensure_loaded and recognizer_transcribe retain their existing public signatures
- [x] #3 FluidAudioBridge releases its Swift handle on Drop (verified or added)
- [ ] #4 After 5+ min idle, next dictation re-enters PHASE_LOADING_MODEL and succeeds — **awaiting user manual smoke; not CI-testable**
- [x] #5 cargo check clean on aarch64-apple-darwin
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
- Commit: 4ec7b9b.
- `core::recognizer::ENGINE` is now `OnceLock<ModelHandle<Box<dyn Recognizer>>>` constructed via `ModelHandle::with_idle_timeout("recognizer", loader, RECOGNIZER_IDLE_TIMEOUT)`. `RECOGNIZER_IDLE_TIMEOUT = 5 min` per spec. Loader closure builds the platform default backend AND `ensure_loaded`s it before returning `Ok(backend)` — the handle's `Loaded` state is the same point Mac's old `ensure_loaded()` would have returned.
- Public surface unchanged: `recognizer_ensure_loaded()` calls `engine().load()`; `recognizer_transcribe(samples)` calls `engine().use_with(|r| r.transcribe(samples))??` (double `?` flattens use_with's `Result<closure-result, String>` and the closure's own `Result<TranscribeResult, String>`).
- `active_ep()` now uses the new `ModelHandle::try_inspect` non-loading peek (added in this task) — diagnostics readouts must not trigger a 200-500 ms cold load just to render an EP label. Returns `None` while `Unloaded`/transitioning, matching the prior "engine not initialized" behavior.
- `FluidAudioBridge::Drop` calls `fab_unload` (new Swift `@_cdecl` export). Without this, `ModelHandle::unload()` drops the empty Rust struct but the Swift global state (`state.asr`, `state.loaded`) keeps the AsrManager + .mlmodelc resident — defeating the idle timer's purpose. `fab_unload` is idempotent (locks the BridgeState NSLock, nils the manager, flips loaded=false).
- New helper `core::recognizer::engine_state() -> Option<LifecycleState>` exposes the recognizer's lifecycle state for future Diagnostics UI consumption (TASK-62.7 will surface this via Tauri telemetry commands).
- AC #4 deferred to user manual QA — the 5-min cold-reload behavior is not CI-testable on a real Parakeet model in reasonable time. Manual repro: launch Tauri dev build, dictate, leave idle ≥ 6 min, dictate again, observe the second dictation goes through `PHASE_LOADING_MODEL` briefly then succeeds.
- `cargo check -p openwhisper-tauri` and `-p openwhisper-cli` clean. `cargo test -p openwhisper-core --lib` 82/82 green (no recognizer-specific tests added — recognizer is feature-gated and Mac-specific).
<!-- SECTION:NOTES:END -->
