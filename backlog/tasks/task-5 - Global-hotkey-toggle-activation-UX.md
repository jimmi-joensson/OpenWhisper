---
id: TASK-5
title: Global hotkey + toggle activation UX
status: To Do
assignee: []
created_date: '2026-04-22 21:11'
updated_date: '2026-04-22 21:18'
labels:
  - macos
  - ux
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Register configurable global hotkey using a toggle semantic (Superwhisper-style): press once to start recording + show pill, press the same hotkey again to stop recording, run transcription, and inject text into the focused input. Default keybinding should match Superwhisper's factory default for familiarity — confirm the exact chord against a current Superwhisper install at implementation time (docs don't publish it). Must be fully rebindable in settings.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Toggle mode works: first press starts recording + shows pill; second press stops + triggers transcribe + inject
- [ ] #2 Default hotkey matches current Superwhisper factory default (verify against live app before shipping)
- [ ] #3 User can rebind hotkey in settings, including single modifier keys (Fn, Right Option) and double-tap chords
- [ ] #4 Works even when OpenWhisper is not the frontmost app
- [ ] #5 Hotkey registers without Accessibility permission until text injection actually fires (defer permission prompt)
<!-- AC:END -->
