---
id: TASK-30
title: Windows tray menu with phase-aware labels
status: In Review
updated_date: '2026-04-24 21:00'
assignee: []
created_date: '2026-04-24 18:45'
labels:
  - windows
  - ui
dependencies:
  - TASK-25
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Windows equivalent of Mac's menubar dropdown menu (`OpenWhisperApp.swift` status menu). Right-clicking the tray icon opens a menu with phase-aware labels: "Start dictation" when idle, "Stop dictation" when recording, "Cancel" when transcribing. Plus "Open OpenWhisper" and "Quit". Matches the Mac menu item-by-item so the tray menu and menubar menu read as the same control surface.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Right-click on tray icon opens a context menu
- [ ] #2 Top item label changes with phase: "Start dictation" / "Stop dictation" / "Cancel"
- [ ] #3 Top item is disabled during `loading_model` phase (mirrors Mac)
- [ ] #4 "Open OpenWhisper" restores / focuses main window
- [ ] #5 "Quit" is the only in-UI exit path when tray-only mode is active (TASK-26)
- [ ] #6 Menu items call into the same DictationService methods as the main window buttons (no duplicated logic)
- [ ] #7 Menu visual styling uses system-native `MenuFlyout` — no custom drawing
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Attach a `MenuFlyout` to the tray icon from TASK-25. 2. Bind the top item's `Text` and `IsEnabled` to the polled Rust-core phase. 3. Wire click handlers to `DictationService.Toggle()` / `CancelIfRecording()` / `MainWindow.Activate()` / `App.Exit()`. 4. Cross-check label text against `OpenWhisperApp.swift` menu item titles to keep wording identical.
<!-- SECTION:PLAN:END -->
