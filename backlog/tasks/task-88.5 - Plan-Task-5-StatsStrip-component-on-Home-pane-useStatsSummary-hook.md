---
id: TASK-88.5
title: 'Plan Task 5: StatsStrip component on Home pane + useStatsSummary hook'
status: In Review
assignee: []
created_date: '2026-05-06 06:15'
updated_date: '2026-05-06 09:30'
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
- [x] #1 useStatsSummary hook invokes stats_get_summary on mount and re-fetches on every stats_changed event; returns { summary, refresh }
- [x] #2 StatsStrip renders 4 cards in grid grid-cols-4 gap-3 using shadcn Card composition (no styled <div> substitutes)
- [ ] #3 After a dictation, all card values update without manual refresh; WPM changes in Settings recompute Time Saved immediately
- [x] #4 Empty state renders 0/0/0/— with subcaption 'vs. typing' (no link, no number) per spec
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Commit: c50b5fd. tsc clean. OW_PW_PORT=1430 pnpm test:ui → 82/82 (no regression, no new spec — 88.7 deferred). AC #3 partial: WPM-changes-recompute clause depends on TASK-88.3 (deferred to 2nd pass); after-dictation auto-update will be verified by manual smoke. WPM hardcoded to 40 in stats-strip.tsx.

c50b5fd TASK-88.5: hook + StatsStrip + Home wiring; pw 82/82

dc45c7b TASK-88.5 fixup: design-match + idle-silence cap

4a0af62 TASK-88.5 fixup: full-width strip + container-query 4/2/1 reflow
<!-- SECTION:NOTES:END -->
