---
id: TASK-62.11
title: 'Plan Task 1: Diagnostics — OW RSS Breakdown bar (Memory section)'
status: To Do
assignee: []
created_date: '2026-05-07 13:59'
labels:
  - 62-impl
dependencies: []
parent_task_id: TASK-62
ordinal: 61000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 <RSSBreakdownBar /> component exists in apps/tauri/src/components/diagnostics-rss-breakdown.tsx as a pure-presentation prop-driven component
- [ ] #2 Diagnostics → Memory section renders the bar below the existing chart legend, separated by a soft border
- [ ] #3 Estimator function breakdownEstimate(rssMb) lives in apps/tauri/src/lib/use-memory-stats.ts and is unit-tested for: model-loaded case, model-unloaded case, low-RSS clamp
- [ ] #4 Legend renders <segment> <%> · <MB> for each of: Parakeet weights, Audio buffers, App shell, Caches
- [ ] #5 Playwright spec covers presence + percentage-sum invariant (sum within 99–101 with rounding tolerance)
- [ ] #6 ui-discipline pass: shadcn primitives where applicable; no styled <div> reaching for primitive duties
<!-- AC:END -->
