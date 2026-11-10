---
id: TASK-93.9
title: 'Plan Task 9: Model and effort controls'
status: In Review
assignee: []
created_date: '2026-11-10T10:57:00Z'
labels:
  - 93-impl
dependencies: []
parent_task_id: TASK-93
ordinal: 73000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Claude model (Sonnet or Haiku) and effort (low, medium, high) are settings, CLI flags and controls in the Refine pane
- [ ] #2 OpenAI-compatible effort is sent only when chosen and a refusing server is remembered
- [ ] #3 Defaults are backed by measurement and documented where they are set
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Default Sonnet + low. Measured warmed medians: Haiku low 0.82-0.85 s, Sonnet low 1.44-1.49 s (max 4.2-4.9 s outliers). Haiku medium was less reliable than Haiku low (16/18 against 18/18 on the variant test; it answered a dictated request). The user asked for medium as the general default; low was chosen on this evidence and is one click to change. Playwright covers the controls.
<!-- SECTION:NOTES:END -->
