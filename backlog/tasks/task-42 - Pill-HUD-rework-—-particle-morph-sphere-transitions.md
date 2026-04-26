---
id: TASK-42
title: Pill HUD rework — particle morph + sphere transitions
status: To Do
assignee: []
created_date: '2026-04-26 19:44'
labels:
  - tauri
  - ui
  - animation
  - pill
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Replace static three-state pill with single 12-particle SVG morph system per design system handoff (claude.ai/design bundle, frame: Pill HUD — three states, live).

Three states retained (idle dots / recording bars / transcribing halftone sphere) but now driven by a single 12-particle render with seamless tweens between layouts. Sphere is 12 fibonacci-lattice points on a unit sphere, y-axis rotation, 2D-projected with depth-driven size + opacity, plus asymmetric inflate/deflate breathing pulse.

Sphere transitions choreograph through a center-pinch waypoint: into-sphere = implode → 80ms hold → spring-out (easeOutBack ~10% overshoot); out-of-sphere = +12% anticipation puff → implode → hold → spring-out. Non-sphere transitions = single-pose tween, 520ms, easeOutQuint pos / easeOutBack size / easeInOutQuart opacity, 0–80ms per-particle stagger. Outer capsule width animates in lockstep — holds at fromWidth through implode (0–0.45), eases to target across hold + explode (0.45–1) — keeps dots inside both directions.

Per-state outer width (height stays 22, radius 11):
- idle: 38px
- recording: 70px
- transcribing: 38px

Material + tokens unchanged from existing PillOverlay.css (rgba(0,0,0,0.55) + backdrop-filter blur, 1px white-8% inner border, 0 4px 14px shadow, reduced-transparency flat fallback).

Implementation strategy (confirmed with user):
- Vanilla SVG, NOT Three.js / Framer Motion / Canvas — 12 dots on a 70x22 surface; render cost is noise vs the host backdrop blur.
- Animation state lives in refs, RAF mutates DOM `<rect>` attrs + capsule `.style.width` directly. Zero React reconciliation per animation frame (Reanimated worklet model). React tree only rerenders on actual status transitions, never per tick.
- Particle position via `transform=translate(x y)` on `<rect>`, not x/y attrs (composited path).
- `will-change: width` on capsule.
- No CSS `transition:` on dots — RAF owns it.

Scope = Tauri only (apps/tauri/src/PillOverlay.tsx + PillOverlay.css). Cross-platform Mac + Windows. apps/macos pill stays as-is per TASK-41 retirement plan.

Reference impl: components.jsx PillOverlay() in design bundle (extracted to /tmp/owdesign/openwhisper/project/components.jsx).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Single 12-particle SVG render replaces branched static markup
- [ ] #2 RAF loop mutates DOM via refs only — verify no React rerender per frame in DevTools profiler
- [ ] #3 Idle ↔ recording transition smooth (520ms, easeOutQuint/easeOutBack/easeInOutQuart, per-particle 0–80ms stagger)
- [ ] #4 Into-sphere transition: implode → 80ms hold → spring-out with ~10% overshoot, 820ms total
- [ ] #5 Out-of-sphere transition: +12% anticipation puff → implode → hold → spring-out, 820ms total
- [ ] #6 Outer capsule width holds at fromWidth through implode (0–0.45), eases to target across hold+explode (0.45–1) — dots never clip outside capsule in either direction
- [ ] #7 Sphere breathing pulse: asymmetric inflate (0.12) + deflate (0.16) gains, dot size scales with pulse, y-rotation at 2× base
- [ ] #8 Material + tokens unchanged from current PillOverlay.css
- [ ] #9 Click-through flips immediately on status change (existing behavior preserved)
- [ ] #10 60fps on macOS M-series and Windows 11 with backdrop blur active
- [ ] #11 Playwright UI smoke (pnpm test:ui) passes
<!-- AC:END -->
