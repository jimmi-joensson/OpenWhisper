---
id: TASK-72
title: Icon micro-animation spike (Josh Comeau patterns applied to lucide icons)
status: To Do
assignee: []
created_date: '2026-05-01 19:54'
labels: []
dependencies: []
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Spike: prototype 3-5 icon micro-animations across T2 surfaces (record-button, sidebar-nav, mic-glyph, window-controls, settings back-arrow) using Josh W Comeau's whimsy patterns (press scale, hover lift, stroke-draw on mount, state morph). Validate which patterns survive the OpenWhisper aesthetic (Mac-style restraint) and which read as noise. Output: a small set of approved icon-motion primitives + Backlog tasks to apply them, plus rejected patterns documented so we don't re-litigate.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Prototype branch contains at least 4 icon micro-animations applied to T2 lucide icons (record-button, sidebar-nav active, mic-glyph press, settings back-arrow hover, or similar)
- [ ] #2 Each prototype uses transform/opacity only, custom cubic-bezier or spring, gated by (hover: hover) where applicable, and respects prefers-reduced-motion
- [ ] #3 Side-by-side video or screenshot comparison (before/after) committed to backlog/docs/specs/2026-05-01-icon-micro-animations.md
- [ ] #4 Findings doc captures: approved primitives (e.g. 'press scale 0.92 over 120ms ease-out'), rejected patterns + reason, recommended rollout order across remaining T2/T3 icons
- [ ] #5 Approved primitives extracted into reusable CSS classes or a single utility hook (apps/tauri/src/lib/icon-motion.ts or similar)
- [ ] #6 Up to 3 follow-up Backlog tasks created for rolling approved primitives across the rest of the icon surface; rejected patterns stay in the doc for future reference
- [ ] #7 Spike runs against openwhisper-animation-philosophy skill: every prototype's tier and decision-flow answers documented in the findings doc
<!-- AC:END -->
