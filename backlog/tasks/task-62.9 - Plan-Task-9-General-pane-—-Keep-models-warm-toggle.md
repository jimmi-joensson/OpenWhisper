---
id: TASK-62.9
title: 'Plan Task 9: General pane — Keep models warm toggle'
status: Done
assignee: []
created_date: '2026-04-30 22:26'
updated_date: '2026-05-07 22:21'
labels:
  - 62-impl
dependencies: []
parent_task_id: TASK-62
ordinal: 12000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Toggle renders in General pane and reflects persisted state on open
- [x] #2 Flip persists to settings.json AND flips the atomic in the same call
- [x] #3 Default OFF for new users
- [x] #4 Visual treatment matches existing pane toggles
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
- New "Performance" section in `general-pane.tsx`, sitting between Stats and Updates. Single horizontal `Field` row; shadcn Switch bound to local `keepModelsWarm` state. Default is `false` — both the React fallback (catch handler) and the Rust `PerformanceSettings::Default` agree, so a fresh install + first paint never flickers ON.
- Mount hydrates via `invoke<PerformanceSettings>("settings_get_performance")`. Rejection path falls back to `false` (same pattern as the existing `settings_get_pill` hydrate above).
- Flip is optimistic: local state → `invoke("settings_set_keep_models_warm", { value })`. Rejection reverts the Switch + console.warns. The Tauri command (TASK-62.4) atomically writes JSON, flips the lock-free `KEEP_MODELS_WARM`, and calls `model_lifecycle::apply_keep_warm` to push the new effective timeout into every registered handle. No restart needed.
- Visual: same primitives + spacing as the Pill / Behavior / Stats toggles in the same pane. Description copy: "Keep speech-recognition (and future cleanup) models in memory between sessions. Uses more RAM, eliminates first-use load delay." Mentions the future cleanup model so the toggle's blast radius is honest once TASK-63 lands.
- Awaiting user QA — `pnpm dev:tauri` smoke: flip toggle ON, dictate, leave 6+ minutes idle, dictate again — Recognizer should NOT re-enter PHASE_LOADING_MODEL on the second dictation. Flip OFF + repeat — second dictation re-enters PHASE_LOADING_MODEL after the 5-minute idle.
- Commit: 38715a5.
<!-- SECTION:NOTES:END -->
