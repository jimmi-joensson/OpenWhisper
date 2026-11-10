---
id: TASK-95.5
title: 'Plan Task 5: Home list, Settings section and quick switches'
status: In Review
assignee: []
created_date: '2026-11-10T14:40:00Z'
updated_date: '2026-11-10T15:07:00Z'
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
Changed after the user tried it in the Dev app on 2026-11-10:
- Home shows the newest 5 entries with a link to a new History screen (sidebar item) that has search, pages of 25, clear all.
- With history on, the dictation that just finished is the first entry of the list and carries the dictionary fixes and Refine marker; with history off the latest transcript keeps its own row.
- Each Home card (Refine, History) has a shortcut to its settings.
- Clear all lives on the History screen, not in Settings.
- History hooks are on TanStack Query (TASK-101 covers moving the rest).
- One shared hover-reveal pattern (.ow-hover-host / .ow-hover-reveal in App.css) now used by history rows, the latest row and the crash list. The user saw row buttons stay visible after hover in the Dev app; not reproduced in Chromium or WebKit under Playwright except after a click in Chromium. The new pattern uses visibility and :focus-visible. Needs the user's confirmation in the Dev app.
ACs 1 to 3 describe the first version of the screen; left unchecked until the user has accepted the new layout.
<!-- SECTION:NOTES:END -->
