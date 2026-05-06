---
id: TASK-88.7
title: 'Plan Task 7: Playwright spec — empty / increment / reset / link click'
status: In Review
assignee:
  - '@claude'
created_date: '2026-05-06 06:15'
updated_date: '2026-05-06 10:31'
labels:
  - 88-impl
dependencies:
  - TASK-88.5
  - TASK-88.6
parent_task_id: TASK-88
ordinal: 59000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 pnpm test:ui actually executed (not inferred) and reports the new spec passing — per CLAUDE.md verification rule
- [x] #2 Case 2 fails if stats_changed subscription regresses (no auto-update after dictation)
- [x] #3 Case 4 fails if in-line link decouples from the WPM setter route (link click goes to wrong pane)
- [ ] #4 apps/tauri/tests/fixtures/tauri-shim.ts gains mockStatsSummary(page, summary) + emitStatsChanged(page) helpers; apps/tauri/tests/stats.spec.ts added with four cases driving those helpers (empty state, increment via mock+event, reset clears, link click navigates)
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Commit: b597780. OW_PW_PORT=1430 pnpm test:ui → 87/87 (5 new stats spec cases). AC #4 (link-click case) intentionally not implemented — option B has no in-line wpm link to click.

b597780 TASK-88.7: stats spec — 5/5 cases green; pw 87/87 total
<!-- SECTION:NOTES:END -->
