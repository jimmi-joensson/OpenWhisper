---
id: TASK-97.4
title: 'Plan Task 4: Shell commands, ask tick, answer window'
status: In Review
assignee: []
created_date: '2026-11-10T17:37:00Z'
updated_date: '2026-11-10T23:02:00Z'
labels:
  - 97-impl
dependencies: []
parent_task_id: TASK-97
ordinal: 103000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 answer window exists in both config files and the capability list, hidden at start, shown above the pill when an ask starts, never key on macOS
- [x] #2 ask_insert hands the finished answer to the injector and hides the panel; without a finished answer it does nothing
- [x] #3 Dismissing while a request runs cancels it
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
255856c shell commands in apps/tauri/src-tauri/src/ask.rs; ask_tick on every snapshot version change and every 250 ms while busy; answer window 420 x 360 pt, centered on the pill's monitor, 12 pt above the capsule; macOS non-activating floating NSPanel, Windows focusable false with SW_SHOWNOACTIVATE. ask_insert also dismisses the ask. Not verified, needs a person: the window showing, its position, focus staying in the other app, insert landing there, fullscreen Spaces, Windows. AC 1 stays unchecked until seen.
<!-- SECTION:NOTES:END -->
