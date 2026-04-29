---
id: TASK-58.2
title: >-
  Plan Task 2: Detector callback honors setting; abort recording on
  fullscreen-entry
status: To Do
assignee: []
created_date: '2026-04-29 18:05'
labels:
  - 58-impl
dependencies: []
parent_task_id: TASK-58
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Fullscreen callback reads behavior::show_in_fullscreen() and short-circuits when true
- [ ] #2 Mid-recording fullscreen entry with setting=false aborts the recording silently — no transcript, no paste, returns to idle
- [ ] #3 Toggling the setting while fullscreen is currently active immediately reconciles pill + hotkey state without restarting OW
- [ ] #4 cargo check clean; manual smoke passes for the four cases (off+idle, off+recording, on+idle, toggle-on-during-fullscreen)
<!-- AC:END -->
