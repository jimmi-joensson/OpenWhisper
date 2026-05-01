---
id: TASK-70.2
title: 'Plan Task 2: Spring-driven scale tween in PillOverlay'
status: To Do
assignee: []
created_date: '2026-05-01 19:16'
updated_date: '2026-05-01 19:38'
labels:
  - 70-impl
dependencies: []
parent_task_id: TASK-70
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 PILL_SCALE map, SPRING_GROW/SPRING_SHRINK configs, and scaleStateRef ({x,v}) exist in PillOverlay.tsx
- [ ] #2 RAF tick computes 2nd-order spring step with real dt (clamped to <=33ms) and snaps when |target-x|<5e-4 and |v|<5e-3
- [ ] #3 .pill-capsule has transform-origin: 50% 100% and will-change: width, transform
- [ ] #4 Manual smoke on Mac: idle->recording shows subtle overshoot; recording->idle is critically damped (no overshoot)
- [ ] #5 Interruption smoke on Mac: retargeting mid-spring carries velocity through direction reversal with no visible jolt
- [ ] #6 Manual smoke on Windows: same spring behavior, no clipping at window edges
<!-- AC:END -->
