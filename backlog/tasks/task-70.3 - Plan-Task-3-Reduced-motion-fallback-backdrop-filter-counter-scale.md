---
id: TASK-70.3
title: 'Plan Task 3: Reduced-motion fallback + backdrop-filter counter-scale'
status: To Do
assignee: []
created_date: '2026-05-01 19:16'
updated_date: '2026-05-01 19:38'
labels:
  - 70-impl
dependencies: []
parent_task_id: TASK-70
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Honor prefers-reduced-motion (snap to target, skip spring + width tween). Counter-scale backdrop-filter via CSS custom property so blur is visually constant in screen pixels across scale tween.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 prefersReducedMotionRef subscribes to media-query changes and unsubscribes on unmount
- [ ] #2 Reduced-motion branch snaps scaleStateRef.x and widthRef.current to target instantly (no spring, no width tween) but particle pose tweens still run
- [ ] #3 .pill-capsule uses var(--pill-blur, 20px) for backdrop-filter and -webkit-backdrop-filter
- [ ] #4 RAF writes --pill-blur per frame as 20/scale, denominator clamped at 0.001
- [ ] #5 Manual smoke: with reduced-motion enabled, status changes are instant; with reduced-motion off, blur disk is visually constant across scale 1<->2
<!-- AC:END -->
