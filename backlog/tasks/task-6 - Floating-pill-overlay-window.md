---
id: TASK-6
title: Floating pill overlay window
status: Done
assignee: []
created_date: '2026-04-22 21:11'
updated_date: '2026-04-23 18:16'
labels:
  - macos
  - ui
dependencies: []
priority: high
ordinal: 8000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Borderless NSPanel at .floating level. Shows audio level animation + state (recording/processing/done). Positionable via settings (presets: top-center, bottom-center, near-cursor, custom coordinates like Superwhisper).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Pill appears on activation, dismisses after transcription completes
- [x] #2 Pill positioned at bottom-center of the active screen, just above the Dock (hard-coded for MVP; configurable in TASK-11)
- [x] #3 Pill stays above all windows including full-screen apps
- [x] #4 Pill shows recording state (mic indicator) and animates with live audio level
- [x] #5 Pill transitions to a distinct 'transcribing' state after stop, then hides after injection
- [x] #6 Pill ignores mouse clicks so the user can keep working in the app behind it
<!-- AC:END -->
