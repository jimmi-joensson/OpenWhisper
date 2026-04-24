---
id: TASK-27
title: 'Windows tap-not-hold hotkey via low-level keyboard hook'
status: To Do
assignee: []
created_date: '2026-04-24 18:45'
labels:
  - windows
  - input
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Bring Windows hotkey semantics in line with Mac's Right-Command tap-not-hold contract (see `apps/macos/App/HotkeyService.swift:145–227`). Today the Windows shell uses `RegisterHotKey` with Left Ctrl+Space — a chord, not a single-modifier tap. Mac semantics: modifier pressed alone → flag set, any intervening keypress cancels the flag, modifier released with flag still set → toggle fires. Requires a `WH_KEYBOARD_LL` low-level keyboard hook because `RegisterHotKey` cannot express "modifier released alone." Default key: Right Ctrl (Right Alt is intercepted by IMEs on some locales, Right Win collides with OS shortcuts). Must remain fully rebindable.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Default activation: Right Ctrl tap-alone (press → release with no intervening key) toggles dictation
- [ ] #2 Pressing any other key while Right Ctrl is held cancels the toggle for that press
- [ ] #3 Hook runs on a dedicated high-priority thread so main-thread stalls don't drop key events
- [ ] #4 Hook uninstalled cleanly on app exit (no orphan hooks across crashes — best-effort)
- [ ] #5 Existing `RegisterHotKey` path removed or feature-flagged off
- [ ] #6 Rebinding API exists at the service level (UI for rebinding is future work, same as Mac)
- [ ] #7 Compatible with common AV software (document any known incompatibilities)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Replace `GlobalHotkey.cs` chord approach with a `LowLevelKeyboardHook` class using `SetWindowsHookExW(WH_KEYBOARD_LL, …)`. 2. Mirror `HotkeyService.swift` state machine: `modifierDown` + `otherKeyPressedWhileHeld` flags, fire on release-alone. 3. Keep callback work minimal — forward to a `Channel<KeyEvent>` consumed on a worker thread so the hook returns fast (Windows will unload slow hooks). 4. Test: Right Ctrl alone toggles; Right Ctrl+C stays a copy; rapid double-tap doesn't double-fire. 5. Document: low-level hooks can be rejected by kernel-mode AV; have a fallback path to chord-style `RegisterHotKey` if hook install fails.
<!-- SECTION:PLAN:END -->
