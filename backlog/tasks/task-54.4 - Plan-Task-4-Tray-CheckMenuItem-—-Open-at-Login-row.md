---
id: TASK-54.4
title: 'Plan Task 4: Tray CheckMenuItem — Open at Login row'
status: To Do
assignee: []
created_date: '2026-04-29 17:44'
labels:
  - 54-impl
dependencies: []
parent_task_id: TASK-54
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Tray right-click menu has Open at Login check item between Toggle Dictation and Preferences…
- [ ] #2 Check reflects is_enabled() on first show + after every autostart_changed
- [ ] #3 Clicking the item flips the plugin state and broadcasts to other surfaces (Settings React Switch reflects within one render)
- [ ] #4 In dev builds the item renders disabled
- [ ] #5 cargo check clean
<!-- AC:END -->
