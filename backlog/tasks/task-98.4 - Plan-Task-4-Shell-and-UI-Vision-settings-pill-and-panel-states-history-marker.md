---
id: TASK-98.4
title: >-
  Plan Task 4: Shell and UI Vision settings, pill and panel states, history
  marker
status: In Review
assignee: []
created_date: '2026-11-10T23:24:00Z'
updated_date: '2026-11-11T06:37:00Z'
labels:
  - 98-impl
dependencies: []
parent_task_id: TASK-98
ordinal: 109000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Trigger can be set; phrases and never-capture apps can be added, removed and restored
- [x] #2 Panel and pill show capturing, included and the reason when there is no screenshot
- [x] #3 Permission row shows the state and what to do
- [x] #4 History marks asks that included the screen
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
662713f shell commands (vision_get, vision_set, vision_request_permission, vision_open_settings, vision_running_apps; a test pins that list and that no shell file names core's capture functions), Screen section in the Ask pane, panel and pill states, history marker. Hooks on TanStack Query. tests/vision.spec.ts 41 tests. Not verified: the real permission prompt, the System Settings pane, the pill label in the real window.
<!-- SECTION:NOTES:END -->
