---
id: TASK-6
title: Floating pill overlay window
status: To Do
assignee: []
created_date: '2026-04-22 21:11'
labels:
  - macos
  - ui
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Borderless NSPanel at .floating level. Shows audio level animation + state (recording/processing/done). Positionable via settings (presets: top-center, bottom-center, near-cursor, custom coordinates like Superwhisper).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Pill appears on activation, dismisses after transcription completes
- [ ] #2 At least 4 position presets available
- [ ] #3 Pill stays above all windows including full-screen apps
- [ ] #4 Animated waveform matches live mic input
<!-- AC:END -->
