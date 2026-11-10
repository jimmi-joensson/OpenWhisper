---
id: TASK-95.5
title: 'Plan Task 5: Home list, Settings section and quick switches'
status: In Review
assignee: []
created_date: '2026-11-10T14:40:00Z'
updated_date: '2026-11-10T15:28:00Z'
labels:
  - 95-impl
dependencies: []
parent_task_id: TASK-95
ordinal: 90000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Home shows the list grouped by day with search, copy, delete, show original, load more and clear all
- [ ] #2 Off and empty states explain what switching it on stores and where
- [ ] #3 General settings has the switch, the retention select, the file location and clear
- [x] #4 Home quick switches for Refine and History (TASK-100)
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Screens as changed with the user on 2026-11-10:
- Home: quick switches (Refine, History) with a settings shortcut each; the newest 5 dictations; link to the History screen.
- With history on, the dictation that just finished is the first entry and carries the dictionary fixes and Refine marker; with history off the latest transcript keeps its own row.
- History screen (sidebar): folder path with Open folder, search, pages of 25, delete, show original, clear all.
- Settings, General, History: switch, retention, folder with Open, Change and Use default, and a sync warning for a chosen folder.
- History hooks on TanStack Query (TASK-101 covers the rest).
- One shared hover-reveal pattern (.ow-hover-host / .ow-hover-reveal) for history rows, the latest row and the crash list. The user saw row buttons stay visible after hover in the Dev app; reproduced only after a click in Chromium. Needs the user's confirmation.
ACs 1 to 3 describe the first version; left unchecked until the user has accepted the screens.
<!-- SECTION:NOTES:END -->
