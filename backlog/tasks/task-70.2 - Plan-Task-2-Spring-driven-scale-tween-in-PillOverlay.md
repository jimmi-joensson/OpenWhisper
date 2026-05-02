---
id: TASK-70.2
title: 'Plan Task 2: Spring-driven scale tween in PillOverlay'
status: In Review
assignee:
  - '@claude'
created_date: '2026-05-01 19:16'
updated_date: '2026-05-02 08:54'
labels:
  - 70-impl
dependencies: []
parent_task_id: TASK-70
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 PILL_SCALE map, SPRING_GROW/SPRING_SHRINK configs, and scaleStateRef ({x,v}) exist in PillOverlay.tsx
- [x] #2 RAF tick computes 2nd-order spring step with real dt (clamped to <=33ms) and snaps when |target-x|<5e-4 and |v|<5e-3
- [x] #3 .pill-capsule has transform-origin: 50% 100% and will-change: width, transform
- [ ] #4 Manual smoke on Mac: idle->recording shows subtle overshoot; recording->idle is critically damped (no overshoot)
- [ ] #5 Interruption smoke on Mac: retargeting mid-spring carries velocity through direction reversal with no visible jolt
- [ ] #6 Manual smoke on Windows: same spring behavior, no clipping at window edges
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
9295233 PILL_SCALE map + SPRING_GROW (k=220,c=24) + SPRING_SHRINK (k=280,c=34) + scaleStateRef ({x,v}) + prevTickRef + prevScaleWriteRef added to PillOverlay.tsx.
<!-- SECTION:NOTES:END -->
