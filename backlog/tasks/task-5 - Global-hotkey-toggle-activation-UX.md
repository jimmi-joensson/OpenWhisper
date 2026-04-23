---
id: TASK-5
title: Global hotkey + toggle activation UX
status: Done
assignee: []
created_date: '2026-04-22 21:11'
updated_date: '2026-04-23 18:16'
labels:
  - macos
  - ux
dependencies: []
priority: high
ordinal: 3000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Register configurable global hotkey using a toggle semantic (Superwhisper-style): press once to start recording + show pill, press the same hotkey again to stop recording, run transcription, and inject text into the focused input. Default keybinding should match Superwhisper's factory default for familiarity — confirm the exact chord against a current Superwhisper install at implementation time (docs don't publish it). Must be fully rebindable in settings.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Toggle mode works: first press starts recording + shows pill; second press stops + triggers transcribe + inject
- [ ] #2 User can rebind hotkey in settings, including single modifier keys (Fn, Right Option) and double-tap chords
- [x] #3 Works even when OpenWhisper is not the frontmost app
- [ ] #4 Hotkey registers without Accessibility permission until text injection actually fires (defer permission prompt)
- [x] #5 Uses CGEventTap + Accessibility (same grant needed for text injection) — single TCC prompt, not two, matching Superwhisper's UX. Replaces the earlier 'defer Accessibility' AC, which assumed an NSEvent-based hotkey implementation.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Runtime verified 2026-04-23. Right Command tap anywhere on the system toggles recording; chord detection filters out Cmd+Q etc. Debug panel (AX trusted / tap status / events seen / last event) shipped in-app to aid dev diagnosis — worth keeping at least through TASK-11 settings work. TCC/Accessibility invalidates on every Debug rebuild (ad-hoc sig); scripts/reset-tcc.sh is the recovery. Rebinding is deferred to TASK-11 (settings window).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Right Command hotkey toggles dictation globally via CGEventTap. Transcript from first end-to-end run: 'Amazing. It is working. So uh we are currently recording from hitting my uh um right command key on on my Mac.' Confidence 0.973.
<!-- SECTION:FINAL_SUMMARY:END -->
