---
id: TASK-35
title: Tauri Phase 4 — Tray + global hotkey + Escape hook
status: Done
assignee: []
created_date: '2026-04-24 22:07'
updated_date: '2026-04-30 16:31'
labels:
  - tauri
  - phase-4
  - hotkey
  - tray
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Three platform integrations in one phase:

1. Tray icon — Tauri 2 built-in tray module. Mono idle, orange when recording. Tooltip reflects phase. Double-click opens main window. Context menu with Open / phase-aware dictation item / Quit. Reference apps/macos/App/OpenWhisperApp.swift for Mac menu wording.

2. Global hotkey — Windows: tauri-plugin-global-shortcut registers Ctrl+Space chord. Mac: custom Rust module using the core-graphics crate to install a CGEventTap replicating HotkeyService.swift (Right Cmd tap-not-hold). Per feedback_hotkey_per_platform.md, do NOT unify these.

3. Escape-to-cancel — minimal low-level key hook per platform. Windows reference: apps/windows/OpenWhisper/Hotkey/EscapeHook.cs. Mac: CGEventTap hook scoped to recording phase.

TCC entitlements (mic, Accessibility, Input Monitoring) need to be wired into the Mac Tauri bundle this phase. Expect TCC grants to drift on ad-hoc resign — plumb an equivalent of scripts/reset-tcc.sh.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Tray icon swaps between mono idle and orange recording; tooltip reflects current phase
- [ ] #2 Double-click tray icon opens main window; right-click shows context menu with Open / phase-aware dictation item / Quit
- [ ] #3 Windows: Ctrl+Space chord registered via tauri-plugin-global-shortcut; starts/stops dictation
- [ ] #4 Mac: Right Cmd tap-not-hold implemented via CGEventTap Rust module; matches HotkeyService.swift semantics
- [ ] #5 Escape-to-cancel works during recording on both OSes; phase-gated in core::dictation
- [ ] #6 Mac Tauri bundle declares Microphone + Accessibility + Input Monitoring entitlements
- [ ] #7 Dev-cycle script exists for Mac resetting TCC grants on rebuild (equivalent of scripts/reset-tcc.sh)
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Closed during 2026-04-30 backlog review. Tauri tray, Windows global-shortcut chord, Mac CGEventTap Right-Cmd hook, and Escape-to-cancel all ship in v0.4.0. TCC dev-loop pain handled via scripts/dev-run.sh + TASK-48 auto-reset.
<!-- SECTION:FINAL_SUMMARY:END -->
