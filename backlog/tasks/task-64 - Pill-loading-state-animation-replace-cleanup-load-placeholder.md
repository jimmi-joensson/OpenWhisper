---
id: TASK-64
title: Pill loading-state animation (replace cleanup-load placeholder)
status: To Do
assignee: []
created_date: '2026-04-30 22:17'
updated_date: '2026-05-04 08:03'
labels: []
dependencies: []
documentation:
  - backlog/docs/plans/2026-05-01-pill-loading-animation.md
priority: medium
ordinal: 3000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Replace the placeholder loading state added in the LLM disfluency cleanup parent with a real animation. Shown when a model is in Loading state (cold reload after idle release). Mac is source of truth for the visual; Tauri mirrors. Small UX polish — depends on the LLM cleanup parent shipping the placeholder.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Pill renders distinct animation (not text 'Loading model...') while ModelHandle state is Loading
- [ ] #2 Visual consistent with existing pill states (idle / recording / transcribing); identity tokens unchanged
- [ ] #3 Animation respects prefers-reduced-motion
- [ ] #4 Playwright snapshot covers Loading visual
<!-- AC:END -->
