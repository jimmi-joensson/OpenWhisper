---
id: TASK-37
title: Tauri Phase 6 — Close-to-tray + single-instance + health banner + auto-update
status: To Do
assignee: []
created_date: '2026-04-24 22:07'
labels:
  - tauri
  - phase-6
  - polish
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Shipping hygiene.

1. Close-to-tray — main window hides on close; app stays alive; only Quit from tray menu exits. Reference apps/macos/App/OpenWhisperApp.swift (LSUIElement behavior + applicationShouldTerminateAfterLastWindowClosed override — project_swiftui_window_lsuielement.md).

2. Single-instance — tauri-plugin-single-instance. Second launch focuses existing instance.

3. Health banner — shown when hotkey registration fails (Windows: Ctrl+Space taken by another app; Mac: Input Monitoring not granted). Inline Retry button re-attempts registration. Reference apps/windows/ health banner implementation from commit 12d01bd.

4. Auto-update — tauri-plugin-updater wired with placeholder endpoint. Verify update flow on both OSes with a mock release.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Closing main window hides it; app process stays alive; only tray-menu Quit exits
- [ ] #2 Launching a second instance focuses the existing window instead of starting a new process
- [ ] #3 Health banner appears in main window when hotkey registration fails; Retry button re-registers
- [ ] #4 tauri-plugin-updater wired; mock update from placeholder endpoint installs successfully on Mac + Windows
<!-- AC:END -->
