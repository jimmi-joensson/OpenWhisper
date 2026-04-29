---
id: TASK-58.1
title: 'Plan Task 1: Settings schema + Rust commands + in-process cache'
status: In Progress
assignee:
  - '@claude'
created_date: '2026-04-29 18:05'
updated_date: '2026-04-29 18:46'
labels:
  - 58-impl
dependencies: []
parent_task_id: TASK-58
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 settings.rs schema includes show_in_fullscreen: bool with default false
- [ ] #2 behavior.rs exposes show_in_fullscreen() reader, set_show_in_fullscreen_cache() writer, and the two Tauri commands
- [ ] #3 behavior_set_show_in_fullscreen persists, updates cache, and emits behavior_show_in_fullscreen_changed with the new boolean
- [ ] #4 Commands registered in generate_handler!; cache hydrated in setup() from loaded settings
- [ ] #5 cargo check clean from apps/tauri/src-tauri/
<!-- AC:END -->
