---
id: TASK-96
title: Modes - named instruction sets with a default per app
status: To Do
assignee: []
created_date: '2026-11-10T14:36:00Z'
labels:
  - modes
  - refine
  - core
  - cli
  - tauri
dependencies:
  - TASK-93
  - TASK-94
priority: medium
ordinal: 78000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Refine has one instructions field today. Turn that into named modes the user can switch between and create: for example plain dictation, a mode that turns loose speech into a precise prompt for a coding agent, a professional tone, translate to English. Each mode is a name plus instructions, and optionally its own model and effort.

On top of manual switching, a mode can be the default for an app: when the focus context says the text goes into that app, its mode is used. This is the step beyond manual mode pickers.

Needs: storage for modes in settings, a switcher (pill, tray or hotkey; decide with the user), a per-app mapping UI fed by apps seen in history, and the active mode recorded on each history entry and Refine run.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Modes can be created, renamed, edited and deleted; one built-in mode keeps today's Refine behaviour
- [ ] #2 The active mode can be switched without opening Settings
- [ ] #3 An app can have a default mode; dictating into that app uses it, and the UI shows which mode was used and why
- [ ] #4 Refine off still means no model call, whatever the mode
- [ ] #5 Headless parity: modes can be listed, set and used from the CLI
- [ ] #6 The live eval set covers at least one non-default mode
<!-- AC:END -->
