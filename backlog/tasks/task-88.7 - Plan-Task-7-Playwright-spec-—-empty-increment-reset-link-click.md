---
id: TASK-88.7
title: 'Plan Task 7: Playwright spec — empty / increment / reset / link click'
status: In Progress
assignee:
  - '@claude'
created_date: '2026-05-06 06:15'
updated_date: '2026-05-06 10:28'
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
- [ ] #1 pnpm test:ui actually executed (not inferred) and reports the new spec passing — per CLAUDE.md verification rule
- [ ] #2 Case 2 fails if stats_changed subscription regresses (no auto-update after dictation)
- [ ] #3 Case 4 fails if in-line link decouples from the WPM setter route (link click goes to wrong pane)
- [ ] #4 apps/tauri/tests/fixtures/tauri-shim.ts gains mockStatsSummary(page, summary) + emitStatsChanged(page) helpers; apps/tauri/tests/stats.spec.ts added with four cases driving those helpers (empty state, increment via mock+event, reset clears, link click navigates)
<!-- AC:END -->
