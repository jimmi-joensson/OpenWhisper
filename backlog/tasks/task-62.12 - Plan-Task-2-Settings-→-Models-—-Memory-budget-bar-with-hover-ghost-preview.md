---
id: TASK-62.12
title: 'Plan Task 2: Settings → Models — Memory budget bar with hover-ghost preview'
status: Done
assignee: []
created_date: '2026-05-07 14:00'
updated_date: '2026-05-07 22:21'
labels:
  - 62-impl
dependencies: []
parent_task_id: TASK-62
ordinal: 62000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 system_physical_ram_mb Tauri command exists and returns the boot-time-cached value
- [x] #2 <ModelsMemoryBudgetBar /> renders at the top of the Settings → Models pane with segments for system+other apps, OpenWhisper base, and each enabled model
- [x] #3 Hovering a disabled model row reveals a striped/dashed ghost segment in the bar AND a +<MB> delta chip on the row's toggle AND swaps the Headroom number to amber <new> (was <old>)
- [x] #4 Hovering an enabled model row marks its existing segment as departing (line-through legend, −<MB> chip, dashed outline) AND swaps the Headroom number to green <new> (was <old>)
- [x] #5 Toggling a model in/out animates the bar segments via the diag-pulse keyframe over 220 ms
- [x] #6 Playwright spec exercises both add-hover and remove-hover paths plus the rest state
- [x] #7 Footer caveat card sits below the budget bar (above the model list) and links the user to Diagnostics → Memory for live values
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented in 71ad482 — adds <ModelsMemoryBudgetBar />, system_physical_ram_mb Tauri cmd, expanded SettingsModelsPane (4-entry catalog), diag-pulse keyframe + 220ms flex-grow toggle transition, ow_navigate('diagnostics') deep link, plugin:event|emit shim handler, and 4 new Playwright cases (117→121). v1 caveat: per-model toggles for Llama/Qwen/Whisper are pane-local placeholders — they don't actually load anything yet. Real toggle wiring lands when non-recognizer model lifecycles ship.

Windows code-side validation pass (2026-05-08): system_physical_ram_mb returns 32717 MB on this 32 GB box (sysinfo total_bytes / 1024 / 1024). Header will read "of 31.95 GB physical" — matches CIM Win32_OperatingSystem.TotalVisibleMemorySize (33502044 KB) and Task Manager's "32 GB" rounded headline. Vitest + Playwright green covering add-hover / remove-hover / rest-state. Caveat link emits ow_navigate "diagnostics" handled by App.tsx:121-128. Live hover-ghost visuals + 220 ms diag-pulse animation still owed by user.
<!-- SECTION:NOTES:END -->
