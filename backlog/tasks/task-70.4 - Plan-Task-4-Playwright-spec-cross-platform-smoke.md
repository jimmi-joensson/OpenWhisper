---
id: TASK-70.4
title: 'Plan Task 4: Playwright spec + cross-platform smoke'
status: In Progress
assignee:
  - '@claude'
created_date: '2026-05-01 19:37'
updated_date: '2026-05-02 07:30'
labels:
  - 70-impl
dependencies: []
parent_task_id: TASK-70
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 pill-overlay.spec.ts exists and passes via pnpm test:ui
- [x] #2 Spec asserts reduced-motion path: status change reaches target rect within ~50 ms via page.emulateMedia({ reducedMotion: 'reduce' })
- [ ] #3 Mac manual smoke: spring grow with subtle overshoot, snap shrink with no overshoot, smooth interruption reversal
- [ ] #4 Windows manual smoke: same behavior, no clipping, RDP reduced-transparency branch still scales
- [x] #5 Existing pill width/sphere tween cadence unchanged at 1x idle (no per-frame transform write when s.x equals target and s.v equals 0)
- [ ] #6 Spec asserts capsule visual dims for idle (38x22), recording (105x33), transcribing (57x33) within +/-1.5 px
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2ef0520 pill-overlay.spec.ts (8 tests) + pillTest fixture + shim handlers for set_pill_click_through and show_main_window. Local 8/8 pass.
<!-- SECTION:NOTES:END -->
