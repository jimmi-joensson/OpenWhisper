---
id: TASK-78.5
title: 'Plan Task 5: Delta-driven launch toast + persistent rail dot + bulk delete'
status: In Progress
assignee:
  - '@claude'
created_date: '2026-05-04 06:16'
updated_date: '2026-05-07 18:04'
labels:
  - 78-impl
milestone: m-1
dependencies: []
parent_task_id: TASK-78
ordinal: 38000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Settings schema gains last_seen_unread_count: u32 (default 0), persisted across restart
- [ ] #2 Launch toast fires only when currentUnread > lastSeenUnread; subsequent restarts at same/lower unread show only the rail dot
- [ ] #3 Rail dot persists until each unread crash is explicitly marked read; never auto-dismissed by route visits, time-out, or toast dismiss
- [ ] #4 Toast 'View' button routes to Diagnostics overview (not the inspector) — entering the inspector requires the user's explicit click on the Crashes entry card, which is the read action
- [ ] #5 Delete-all empties both crash files and state.json entries; confirm dialog uses dynamic count copy
- [ ] #6 crashes_summary command exists and returns the latest crash's relative-when + module + signal for the entry card's sub-line
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
76206ff shared crashes-store via useSyncExternalStore; sidebar rail dot in lockstep with list. Implements rail-dot AC; launch toast / lastSeenUnreadCount / crashes_summary still pending.
<!-- SECTION:NOTES:END -->
