---
id: TASK-58.1
title: 'Plan Task 1: Settings schema + Rust commands + in-process cache'
status: Done
assignee:
  - '@claude'
created_date: '2026-04-29 18:05'
updated_date: '2026-04-29 18:49'
labels:
  - 58-impl
dependencies: []
parent_task_id: TASK-58
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 settings.rs schema includes show_in_fullscreen: bool with default false
- [x] #2 behavior.rs exposes show_in_fullscreen() reader, set_show_in_fullscreen_cache() writer, and the two Tauri commands
- [x] #3 behavior_set_show_in_fullscreen persists, updates cache, and emits behavior_show_in_fullscreen_changed with the new boolean
- [x] #4 Commands registered in generate_handler!; cache hydrated in setup() from loaded settings
- [x] #5 cargo check clean from apps/tauri/src-tauri/
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
352ef8e TASK-58.1 schema + behavior.rs + lib.rs wiring; cargo check clean
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added BehaviorSettings persistence block (mirrors AudioSettings shape), new behavior.rs with AtomicBool cache + behavior_get/set_show_in_fullscreen Tauri commands emitting behavior_show_in_fullscreen_changed, and cache hydration in lib.rs setup(). cargo check clean.
<!-- SECTION:FINAL_SUMMARY:END -->
