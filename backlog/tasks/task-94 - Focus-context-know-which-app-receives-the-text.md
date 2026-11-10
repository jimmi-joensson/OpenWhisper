---
id: TASK-94
title: Focus context - know which app receives the text
status: To Do
assignee: []
created_date: '2026-11-10T14:36:00Z'
labels:
  - focus
  - core
  - cli
  - tauri
dependencies: []
priority: high
ordinal: 76000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
A small module in core that reports what the user is working in at a given moment: app identifier, app display name and window title. First consumer is transcript history (which app a dictation went into). Later consumers: a default mode per app, and the ask, vision and agent features.

macOS: frontmost application (bundle id + localized name) and the focused window title. Windows: foreground window, owning process exe name, product name and window title. Read at insert time, because that is the app that receives the text.

Core owns the type and the read; the Tauri shell only calls it. OpenWhisper's own windows (main, pill) are reported as such so callers can ignore them. No screen capture and no reading of text inside the window in this task.

Open point to settle while building: whether the window title needs a permission beyond the Accessibility grant the app already has on macOS, and what to return when it is not available (empty title, never a failure).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Core exposes one call that returns app id, app name and window title for the frontmost app, on macOS and Windows
- [ ] #2 A missing piece (no title, no permission, unknown process) comes back empty and never fails or delays a dictation
- [ ] #3 The read is fast enough to run on every dictation; measured and written into the task notes
- [ ] #4 OpenWhisper's own windows are flagged so callers can skip them
- [ ] #5 Headless parity: an openwhisper CLI command prints the current focus context
- [ ] #6 Windows path compiles and has tests; run on a real Windows machine before the task leaves In Review
<!-- AC:END -->
