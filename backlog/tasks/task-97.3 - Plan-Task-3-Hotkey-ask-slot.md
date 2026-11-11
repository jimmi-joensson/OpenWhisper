---
id: TASK-97.3
title: 'Plan Task 3: Hotkey ask slot'
status: In Review
assignee: []
created_date: '2026-11-10T17:37:00Z'
updated_date: '2026-11-10T23:02:00Z'
labels:
  - 97-impl
dependencies: []
parent_task_id: TASK-97
ordinal: 102000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Ask slot installed only while Ask is on, with the platform default when unset; calls run_toggle_kind(Ask)
- [x] #2 Cancel slot gate also passes while an ask is running
- [x] #3 Shortcuts shows an Ask row that can be recorded and reset (Playwright)
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
255856c ask slot in the SLOTS table, installed only while Ask is on; cancel gate is dictation::cancel_would_act(). Shell tests 59 + 1, Playwright 252 (orchestrator runs). Not verified: the key on a real keyboard, Windows at run time.
<!-- SECTION:NOTES:END -->
