---
id: TASK-62.10
title: 'Plan Task 10: Playwright spec — diagnostics + keep-warm toggle'
status: Done
assignee: []
created_date: '2026-04-30 22:26'
updated_date: '2026-05-07 22:21'
labels:
  - 62-impl
dependencies: []
parent_task_id: TASK-62
ordinal: 13000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 5 Playwright assertions: sidebar entry, RSS update, state update, toggle flip, hydrate-from-stored
- [x] #2 Manual smoke steps documented in the spec
- [x] #3 pnpm test:ui green locally and on CI
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
- 5 assertions across two spec files: 3 in `diagnostics.spec.ts` (sidebar-entry-opens-Memory-card, RSS-readout-reflects-snapshot, breakdown-bar-adds-segment-on-model-state-changed) + 2 in `settings-window.spec.ts` (keep-warm-flips-ON-with-invoke-recording, keep-warm-hydrates-from-stored).
- The "RSS update" and "state update" plan-ACs collapsed into the two diagnostics tests above: both flow through the same `model-state-changed → refetch` path (the 1-Hz poll uses the same code), so testing the event path covers both. Avoids real-time waits on the `setInterval`.
- Shim extensions in `tauri-shim.ts`: handlers for `telemetry_get_memory`, `settings_get_performance`, `settings_set_keep_models_warm`. Each follows the existing record-on-window pattern (`__owMemoryStats`, `__owKeepModelsWarm`, `__owKeepModelsWarmSetCount`, etc). New helpers: `setMemoryStats`, `emitModelStateChanged`, `waitForModelStateChangedListener`.
- Subtle bug caught during first run: `setMemoryStats` called BEFORE `page.goto("/")` evaluates on the `about:blank` window, which is destroyed by goto — the post-goto window has no fixture. All three diagnostics tests reorder `goto → setMemoryStats → click sidebar`. Same gotcha applies to any future `set*` helper that mutates window state.
- Pre-pivot diagnostics assertions removed: FFI card / Dictation debug card / transcript card / phase-transitions-drive-RecordButton describe block. The RecordButton lived only inside the old diagnostics pane content; Home uses the pill HUD + global hotkey (no in-pane Record button today), so the phase-transition coverage is genuinely orphaned by this rewrite — not worth resurrecting under a synthetic owner.
- `main-window.spec.ts` ported two stale text anchors (`Rust ↔ React FFI`, `transcript`) to the new pane — `Diagnostics` heading + `Memory` card visibility + footer caveat for the scroll test (TASK-62.8 commit, since the change was a side effect of the pane rewrite).
- Manual smoke note at the top of `diagnostics.spec.ts`: real cold-load-after-idle flow (Loaded → Unloaded → first-dictation re-enters PHASE_LOADING_MODEL after 6 min idle) needs a real recognizer load + a 6-minute timer per OS, not CI-testable. Documented with explicit steps for Mac and Windows.
- `pnpm test:ui` 86/86 green. tsc clean. Awaiting user QA on the real flows.
- Commit: 8720c2c.
<!-- SECTION:NOTES:END -->
