---
id: TASK-70
title: Pill scales 2x during recording/transcribing
status: To Do
assignee: []
created_date: '2026-05-01 19:14'
updated_date: '2026-05-01 19:38'
labels: []
dependencies: []
documentation:
  - backlog/docs/specs/2026-05-01-pill-active-scale.md
  - backlog/docs/plans/2026-05-01-pill-active-scale.md
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Capsule + content scale 1x->2x via spring (subtle bounce on grow, critically damped on shrink) when entering recording/transcribing, scale back to 1x on idle. Honors prefers-reduced-motion. Backdrop-filter counter-scaled so material stays visually constant.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Idle pill renders at current 38x22 capsule size
- [ ] #2 Recording pill renders at 2x scale (140x44) with content scaled
- [ ] #3 Transcribing pill renders at 2x scale (76x44) with content scaled
- [ ] #4 Scale uses spring physics (asymmetric: bounce on grow, critically damped on shrink) with bottom-anchored transform-origin
- [ ] #5 Spring is interruptible: retargeting mid-motion carries velocity smoothly into the new direction
- [ ] #6 prefers-reduced-motion snaps to target instantly (no spring, no width tween) for both new scale and existing width animation
- [ ] #7 Backdrop-filter blur visually constant in screen pixels across scale (counter-scaled via CSS custom property)
- [ ] #8 Pill OS window has clearance for 2x capsule + shadow on Mac and Windows
<!-- AC:END -->
