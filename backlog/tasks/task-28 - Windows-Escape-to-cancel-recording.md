---
id: TASK-28
title: Windows Escape-to-cancel recording
status: To Do
assignee: []
created_date: '2026-04-24 18:45'
labels:
  - windows
  - input
dependencies:
  - TASK-27
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Windows equivalent of Mac's global Escape-to-cancel-while-recording. On Mac, Escape posted to the event tap fires `.openWhisperCancelDictation`; DictationService decides whether to act based on phase. Ride the low-level keyboard hook from TASK-27 so we don't install a second hook. Cancel must only affect active recordings — Escape must remain a normal key for every other app.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Escape pressed during recording cancels the session (discards audio, returns to idle, no paste)
- [ ] #2 Escape pressed in any other phase (idle, loading, transcribing) passes through to the focused app
- [ ] #3 Implemented as an additional event in the TASK-27 hook — no second `SetWindowsHookExW` call
- [ ] #4 Gating lives in the Rust core (phase-aware cancel), not in the hook callback
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. In the low-level hook callback (TASK-27), forward `VK_ESCAPE` presses to the shared event channel tagged as `CancelRequested`. 2. Never swallow the Escape key — always return `CallNextHookEx` for Escape, since other apps need it. 3. On the dispatcher, call `DictationService.CancelIfRecording()` — which in turn calls a Rust-core entry point that's no-op outside the recording phase. 4. Cross-check with `HotkeyService.swift` handling of `.openWhisperCancelDictation` to ensure parity.
<!-- SECTION:PLAN:END -->
