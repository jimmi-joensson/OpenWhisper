---
id: TASK-55.1
title: 'Plan Task 1: Settings schema, atomic flag, and commands'
status: To Do
assignee: []
created_date: '2026-04-29 08:01'
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
- [ ] #1 PillSettings type added with follow_active_screen: bool, default true
- [ ] #2 follow_active_screen() returns true on a fresh checkout and on a settings file without the pill block
- [ ] #3 settings_set_pill_follow(false) persists the JSON field AND flips the in-memory atomic in the same call
- [ ] #4 Both Tauri commands registered in invoke_handler!
- [ ] #5 cargo check clean, no new warnings
<!-- AC:END -->
