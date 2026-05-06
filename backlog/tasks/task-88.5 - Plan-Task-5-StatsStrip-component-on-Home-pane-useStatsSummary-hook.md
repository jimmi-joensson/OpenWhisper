---
id: TASK-88.5
title: 'Plan Task 5: StatsStrip component on Home pane + useStatsSummary hook'
status: To Do
assignee: []
created_date: '2026-05-06 06:15'
labels:
  - 88-impl
dependencies:
  - TASK-88.2
  - TASK-88.3
parent_task_id: TASK-88
ordinal: 57000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 useStatsSummary hook invokes stats_get_summary on mount and re-fetches on every stats_changed event; returns { summary, refresh }
- [ ] #2 StatsStrip renders 4 cards in grid grid-cols-4 gap-3 using shadcn Card composition (no styled <div> substitutes)
- [ ] #3 After a dictation, all card values update without manual refresh; WPM changes in Settings recompute Time Saved immediately
- [ ] #4 Empty state renders 0/0/0/— with subcaption 'vs. typing' (no link, no number) per spec
<!-- AC:END -->
