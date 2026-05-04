---
id: TASK-71
title: Audit existing animations + interactive elements (Emil framework pass)
status: To Do
assignee: []
created_date: '2026-05-01 19:25'
labels: []
dependencies: []
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Sweep every animation, transition, hover, press, and focus state across the Tauri shell (pill + main + settings) against Emil Kowalski's design-engineering framework. Produce a findings doc with prioritized fixes; defer implementation to a separate task once findings are scoped.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Findings doc at backlog/docs/specs/2026-05-01-animation-audit.md lists every animation/transition/interactive in the Tauri shell
- [ ] #2 Each entry has: location (file:line), current behavior, framework rule it satisfies or violates, recommended fix, priority (high/med/low)
- [ ] #3 Audit covers: pill state tweens, main window route transitions, settings pane transitions, button :active states, hover states, focus rings, hotkey-rebind UI, transcript-row interactions, sidebar nav, switches/toggles
- [ ] #4 Audit explicitly checks: durations (<300ms target), easing curves (custom > built-in), transform vs layout properties, prefers-reduced-motion coverage, hover gating behind (hover: hover) media query, keyboard-action animations (should be none)
- [ ] #5 Audit cross-references TASK-70 findings (scale duration, asymmetric timing, reduced-motion, backdrop-filter at 2x) so they are not duplicated
- [ ] #6 Top 3 highest-priority fixes captured as separate top-level Backlog tasks; remainder stays in the doc as a backlog of polish work
<!-- AC:END -->
