---
id: TASK-49
title: Settings window — shell + sidebar layout
status: Done
assignee: []
created_date: '2026-04-27 15:28'
updated_date: '2026-04-27 15:41'
labels:
  - ui
  - tauri
  - settings
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add a Settings window scene to the Tauri app, wired with the sidebar layout from the design. Shell only — no panes wired. Establishes the home for Shortcuts, Audio, and General panes (and Models later). Open via tray Preferences… and ⌘, accelerator.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Tauri config defines a 'settings' window (hidden on launch, shows on demand)
- [x] #2 Sidebar matches design (General / Audio / Models / Shortcuts) with routing between empty pane stubs
- [x] #3 Tray Preferences… and ⌘, both open the Settings window and focus it
- [x] #4 Settings window honors close-to-tray (hide on close, not exit) — same pattern as main window
- [x] #5 Sidebar items are keyboard-navigable
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implementation:
- tauri.conf.json defines hidden 'settings' window (720×560, min 640×480).
- capabilities/default.json grants core:window perms to the settings label.
- lib.rs adds open_settings_window cmd + close-to-hide on_window_event.
- tray/mod.rs adds Preferences… item with CmdOrCtrl+, accelerator label.
- main.tsx routes settings label to new Settings.tsx tree.
- Settings.tsx + Settings.css implement sidebar (General/Audio/Models/Shortcuts) with stub panes; ↑/↓ cycle, ⌘W hides.
- App.tsx listens for ⌘, in main window and invokes open_settings_window.
- Caveat: ⌘, accelerator on tray menu item is label-only on macOS (Accessory app, no NSApp.mainMenu). Real keyboard activation works from main + settings windows; tray click works system-wide. App-menu wiring deferred — not needed for shell.
- Tests: 5 new specs in settings-window.spec.ts (sidebar render, landing, click, arrow nav, ⌘, invoke). All 15 UI tests pass; cargo check clean; vite build clean (Settings chunk = 1.75 kB gzipped).
<!-- SECTION:NOTES:END -->
