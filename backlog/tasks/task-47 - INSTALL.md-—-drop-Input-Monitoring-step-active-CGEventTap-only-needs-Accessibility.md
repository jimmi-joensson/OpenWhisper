---
id: TASK-47
title: >-
  INSTALL.md — drop Input Monitoring step (active CGEventTap only needs
  Accessibility)
status: Done
assignee: []
created_date: '2026-04-27 06:26'
updated_date: '2026-04-27 06:38'
labels: []
dependencies: []
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
INSTALL.md (and the AX-prompt fallback copy) tells Mac users to grant Input Monitoring as a third manual permission. That is wrong for the Tauri build:

- `apps/tauri/src-tauri/src/hotkey/mac.rs::run_tap_thread` creates the tap with `CGEventTapOptions::Default` (active tap, can modify events).
- Active taps require **Accessibility** (`kTCCServiceAccessibility`) only. ListenOnly taps require Input Monitoring (`kTCCServiceListenEvent`).
- During the 0.3.0 manual smoke we confirmed: with hardened runtime stripped + AX granted, the hotkey works fine and OpenWhisper never appears in the Input Monitoring list.

Ripple: dev-run.sh resets `ListenEvent` along with `Accessibility`/`Microphone` (apps/tauri/scripts/dev-run.sh:67) — harmless but vestigial.

Scope:
- Remove the "Input Monitoring (manual)" section from INSTALL.md step 3.
- Drop the corresponding troubleshooting line.
- Optionally drop `ListenEvent` from the dev-run.sh tccutil reset loop (low value to keep).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 INSTALL.md no longer instructs users to grant Input Monitoring manually
- [ ] #2 Troubleshooting section is consistent with active-tap-only behavior
- [ ] #3 Smoke: fresh download → drag to /Applications → grant AX only → hotkey + mic prompt work end-to-end
<!-- AC:END -->
