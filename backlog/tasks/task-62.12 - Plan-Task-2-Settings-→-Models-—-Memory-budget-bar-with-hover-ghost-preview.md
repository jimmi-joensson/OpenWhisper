---
id: TASK-62.12
title: 'Plan Task 2: Settings → Models — Memory budget bar with hover-ghost preview'
status: To Do
assignee: []
created_date: '2026-05-07 14:00'
updated_date: '2026-05-07 14:02'
labels:
  - 62-impl
dependencies: []
parent_task_id: TASK-62
ordinal: 62000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 system_physical_ram_mb Tauri command exists and returns the boot-time-cached value
- [ ] #2 <ModelsMemoryBudgetBar /> renders at the top of the Settings → Models pane with segments for system+other apps, OpenWhisper base, and each enabled model
- [ ] #3 Hovering a disabled model row reveals a striped/dashed ghost segment in the bar AND a +<MB> delta chip on the row's toggle AND swaps the Headroom number to amber <new> (was <old>)
- [ ] #4 Hovering an enabled model row marks its existing segment as departing (line-through legend, −<MB> chip, dashed outline) AND swaps the Headroom number to green <new> (was <old>)
- [ ] #5 Toggling a model in/out animates the bar segments via the diag-pulse keyframe over 220 ms
- [ ] #6 Playwright spec exercises both add-hover and remove-hover paths plus the rest state
- [ ] #7 Footer caveat card sits below the budget bar (above the model list) and links the user to Diagnostics → Memory for live values
<!-- AC:END -->
