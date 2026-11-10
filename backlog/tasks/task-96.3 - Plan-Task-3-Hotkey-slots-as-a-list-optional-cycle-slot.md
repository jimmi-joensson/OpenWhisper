---
id: TASK-96.3
title: 'Plan Task 3: Hotkey slots as a list, optional cycle slot'
status: In Review
assignee: []
created_date: '2026-11-10T16:13:00Z'
updated_date: '2026-11-10T17:02:00Z'
labels:
  - 96-impl
dependencies: []
parent_task_id: TASK-96
ordinal: 96000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Both shells hold slots in a list; toggle and cancel behave as before
- [x] #2 Unset cycle slot installs nothing; a set one cycles once per press
- [x] #3 macOS: another modifier pressed during a tap counts as another key
- [x] #4 A cycle binding that cannot be installed is reported and dictation keeps working
- [x] #5 Shortcuts pane shows, captures, resets and clears the third row (Playwright)
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
58c4d7e hotkey slot table. Run by the orchestrator: 37 shell tests, Windows target check clean, 28 unit, 202 Playwright (197 before). Behaviour change for the dictation key on macOS: a modifier tap no longer fires while another modifier is held (Shift held, then Right Command tapped). Caps Lock and Fn are not counted. Not verified: real key presses on macOS, anything on Windows at run time.
<!-- SECTION:NOTES:END -->
