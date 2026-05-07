---
id: TASK-62.11
title: 'Plan Task 1: Diagnostics — OW RSS Breakdown bar (Memory section)'
status: Done
assignee: []
created_date: '2026-05-07 13:59'
updated_date: '2026-05-07 22:21'
labels:
  - 62-impl
dependencies: []
parent_task_id: TASK-62
ordinal: 61000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 <RSSBreakdownBar /> component exists in apps/tauri/src/components/diagnostics-rss-breakdown.tsx as a pure-presentation prop-driven component
- [x] #2 Diagnostics → Memory section renders the bar below the existing chart legend, separated by a soft border
- [x] #3 Estimator function breakdownEstimate(rssMb) lives in apps/tauri/src/lib/use-memory-stats.ts and is unit-tested for: model-loaded case, model-unloaded case, low-RSS clamp
- [x] #4 Legend renders <segment> <%> · <MB> for each of: Parakeet weights, Audio buffers, App shell, Caches
- [x] #5 Playwright spec covers presence + percentage-sum invariant (sum within 99–101 with rounding tolerance)
- [x] #6 ui-discipline pass: shadcn primitives where applicable; no styled <div> reaching for primitive duties
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented in 23f1be2 — adds <RSSBreakdownBar /> + breakdownEstimate() + 7 vitest cases + 2 Playwright cases (113→114 specs). Estimator signature is breakdownEstimate(rssMb, recognizerLoaded) — boolean arg over models[] for cleaner unit tests; recognizer-resident check exposed as separate isRecognizerResident() helper.

Windows code-side validation pass (2026-05-08): cargo -p openwhisper-core green; vitest 22/22; Playwright 121/121 incl. tests/diagnostics.spec.ts:307 (recognizer-load → 4-segment) and :434 (unloaded → Parakeet hidden). isRecognizerInProcessResident path verified — recognizer module reports in_process: cfg!(not(target_os="macos")), so the Windows in-process branch is reached. Live click-through in pnpm dev:tauri:win still owed by user before flipping past In Review.
<!-- SECTION:NOTES:END -->
