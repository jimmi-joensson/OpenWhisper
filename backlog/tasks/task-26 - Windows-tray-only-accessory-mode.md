---
id: TASK-26
title: Windows tray-only / accessory mode
status: In Review
updated_date: '2026-04-24 20:45'
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
Windows analog of macOS `.accessory` activation policy — the app lives in the tray, not the taskbar. Closing the main window minimizes to tray rather than exiting; the hotkey stays active. Matches Mac where closing the main window doesn't kill dictation. Because tray-only is less idiomatic on Windows than on Mac, expose a "Show in taskbar" preference (default off) so power users can opt into a taskbar presence.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Clicking the main window close button hides the window, keeps the app and hotkey alive
- [ ] #2 App terminates only via tray menu "Quit" (TASK-30) or task manager
- [ ] #3 Main window hidden from taskbar while closed; visible in taskbar only while open (default behavior)
- [ ] #4 "Show in taskbar" setting available (off by default); when on, main window shows in taskbar even after close→restore cycles
- [ ] #5 Alt-Tab does not show the main window when it is hidden
- [ ] #6 Single-instance enforcement preserved (second launch focuses existing tray/window rather than spawning)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Intercept `MainWindow.Closed` or `AppWindowClosingEventArgs` → `args.Cancel = true; window.Hide()`. 2. Set `WS_EX_TOOLWINDOW` on the main HWND when "Show in taskbar" is off so it drops out of the taskbar and Alt-Tab. 3. Settings store: a simple JSON at `%LOCALAPPDATA%\OpenWhisper\settings.json` (no full settings UI yet — single boolean, read on startup). 4. Single-instance guard via named mutex; second launch sends a message to activate the existing instance.
<!-- SECTION:PLAN:END -->
