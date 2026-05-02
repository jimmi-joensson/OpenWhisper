---
id: TASK-70
title: Pill scales 2x during recording/transcribing
status: Done
assignee: []
created_date: '2026-05-01 19:14'
updated_date: '2026-05-02 08:55'
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
- [ ] #2 Scale uses spring physics (asymmetric: bounce on grow, critically damped on shrink) with bottom-anchored transform-origin
- [ ] #3 Spring is interruptible: retargeting mid-motion carries velocity smoothly into the new direction
- [ ] #4 prefers-reduced-motion snaps to target instantly (no spring, no width tween) for both new scale and existing width animation
- [ ] #5 Backdrop-filter blur visually constant in screen pixels across scale (counter-scaled via CSS custom property)
- [ ] #6 Recording pill renders at 1.5x scale (105x33) with content scaled
- [ ] #7 Transcribing pill renders at 1.5x scale (57x33) with content scaled
- [ ] #8 Pill OS window has clearance for 1.5x capsule + shadow on Mac and Windows
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Pill capsule scales 1x<->1.5x via hand-rolled spring during recording/transcribing. Asymmetric (subtle overshoot grow, critically damped shrink), velocity-preserving across reversal. prefers-reduced-motion honored (also fixes existing width tween). Backdrop-filter counter-scaled so material stays invariant. Window 130x82 -> 180x110 for paint headroom. Merged in PR #13. Original 2x target dialed to 1.5x after Mac smoke felt too aggressive against the dock.
<!-- SECTION:FINAL_SUMMARY:END -->
