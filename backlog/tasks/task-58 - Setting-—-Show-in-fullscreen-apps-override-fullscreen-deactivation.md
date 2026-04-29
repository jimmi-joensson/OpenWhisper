---
id: TASK-58
title: Setting — Show in fullscreen apps (override fullscreen deactivation)
status: To Do
assignee: []
created_date: '2026-04-29 18:02'
updated_date: '2026-04-29 20:49'
labels:
  - ui
  - tauri
  - settings
dependencies: []
documentation:
  - docs/superpowers/specs/2026-04-29-fullscreen-behavior-toggle.md
  - docs/superpowers/plans/2026-04-29-fullscreen-behavior-toggle.md
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add a core setting that lets the user opt out of OW's automatic deactivation when another app is fullscreen on the focused screen. Default off (= current behavior: pill hides, hotkey suppressed, in-flight recording aborts on fullscreen entry). On = pill stays visible best-effort over the fullscreen app, hotkey stays active. Setting persists in core settings (not localStorage) so the Rust-side detector callback can read it without round-tripping through the WebView.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 New core setting behavior.show_in_fullscreen persists across boot; default false
- [x] #2 Setting=off preserves current behavior — pill hides, hotkey suppressed when foreground app on focused screen is fullscreen
- [x] #3 Setting=off + recording in flight: entering fullscreen aborts the recording silently — no transcript delivered, no paste, returns to idle
- [x] #4 Setting=on: pill stays visible (best-effort) and hotkey stays active when the foreground app is fullscreen; macOS pill collection-behavior allows overlaying fullscreen Spaces
- [x] #5 Settings → General has a Switch row for the toggle; reads/writes via core settings; reflects external changes via emitted event
- [x] #6 Playwright covers the toggle UI; existing Settings + General-pane tests still pass
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
c7d50e6 Subtasks 58.1-58.5 all Done. cargo check clean, pnpm tsc --noEmit clean, pnpm test:ui 43/43 green. AC#2/#3/#4 require manual smoke (release build for setting=on path; dev build for off+idle, off+recording, toggle-on-during-fullscreen) — not verifiable from this shell, deferred to merge-time or user smoke pass.

da26ad0+a982b42 fullscreen overlay verified via tauri-nspanel; learning captured in openwhisper-platform-gotchas + new openwhisper-iteration-budget skill

fad1364 AC#2 + AC#3 verified manually after deferred-hide fix landed. All parent ACs now satisfied.
<!-- SECTION:NOTES:END -->
