---
id: TASK-100
title: Home quick switches - Refine and History without opening Settings
status: In Review
assignee: []
created_date: '2026-11-10T14:39:00Z'
updated_date: '2026-11-10T15:07:00Z'
labels:
  - home
  - refine
  - history
  - tauri
dependencies:
  - TASK-93
  - TASK-95
priority: medium
ordinal: 82000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
A small row on Home with a switch for Refine and one for History, so either can be turned on and off without going to Settings. Asked for on 2026-11-10. Agent mode (TASK-99) gets its own switch here when it exists. The switches read and write the same settings blocks as the Settings panes.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Home shows a Refine switch and a History switch that reflect the saved settings
- [x] #2 Flipping a switch saves at once and the next dictation follows it
- [x] #3 The Settings panes show the same state
- [x] #4 Playwright covers both switches
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
home-switches.tsx. Each card also has a shortcut to its settings (asked for on 2026-11-10): Refine opens the Refine pane, History opens General scrolled to its History section. The Refine switch does not start the backend probe.
<!-- SECTION:NOTES:END -->
