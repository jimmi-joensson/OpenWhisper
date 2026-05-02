---
id: TASK-70.4
title: 'Plan Task 4: Playwright spec + cross-platform smoke'
status: In Progress
assignee:
  - '@claude'
created_date: '2026-05-01 19:37'
updated_date: '2026-05-01 20:06'
labels:
  - 70-impl
dependencies: []
parent_task_id: TASK-70
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 pill-overlay.spec.ts exists and passes via pnpm test:ui
- [ ] #2 Spec asserts capsule visual dims for idle (38x22), recording (140x44), transcribing (76x44) within +/-1 px
- [ ] #3 Spec asserts reduced-motion path: status change reaches target rect within ~50 ms via page.emulateMedia({ reducedMotion: 'reduce' })
- [ ] #4 Mac manual smoke: spring grow with subtle overshoot, snap shrink with no overshoot, smooth interruption reversal
- [ ] #5 Windows manual smoke: same behavior, no clipping, RDP reduced-transparency branch still scales
- [ ] #6 Existing pill width/sphere tween cadence unchanged at 1x idle (no per-frame transform write when s.x equals target and s.v equals 0)
<!-- AC:END -->
