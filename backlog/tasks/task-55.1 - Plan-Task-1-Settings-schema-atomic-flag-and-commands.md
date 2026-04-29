---
id: TASK-55.1
title: 'Plan Task 1: Settings schema, atomic flag, and commands'
status: Done
assignee:
  - '@claude'
created_date: '2026-04-29 08:01'
updated_date: '2026-04-29 18:50'
labels:
  - 55-impl
dependencies: []
parent_task_id: TASK-55
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add pill.follow_active_screen block to settings.json, expose a process-global AtomicBool the watcher can read, and ship the two Tauri commands (settings_get_pill, settings_set_pill_follow) the UI will call.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 PillSettings type added with follow_active_screen: bool, default true
- [x] #2 follow_active_screen() returns true on a fresh checkout and on a settings file without the pill block
- [x] #3 settings_set_pill_follow(false) persists the JSON field AND flips the in-memory atomic in the same call
- [x] #4 Both Tauri commands registered in invoke_handler!
- [x] #5 cargo check clean, no new warnings
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
ee05695 Settings: PillSettings + FOLLOW_ACTIVE_SCREEN atomic + 2 commands; cargo check clean.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added PillSettings (default ON), FOLLOW_ACTIVE_SCREEN AtomicBool, settings_get_pill + settings_set_pill_follow commands. load_settings hydrates the atomic on first call; setter writes JSON + flips atomic atomically. Sibling save fns updated to preserve pill block. cargo check clean.
<!-- SECTION:FINAL_SUMMARY:END -->
