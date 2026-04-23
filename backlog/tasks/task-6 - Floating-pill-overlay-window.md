---
id: TASK-6
title: Floating pill overlay window
status: In Progress
assignee: []
created_date: '2026-04-22 21:11'
updated_date: '2026-04-23 07:44'
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
- [ ] #2 Pill positioned at bottom-center of the active screen, just above the Dock (hard-coded for MVP; configurable in TASK-11)
- [ ] #3 Pill stays above all windows including full-screen apps
- [ ] #4 Pill shows recording state (mic indicator) and animates with live audio level
- [ ] #5 Pill transitions to a distinct 'transcribing' state after stop, then hides after injection
- [ ] #6 Pill ignores mouse clicks so the user can keep working in the app behind it
<!-- AC:END -->
