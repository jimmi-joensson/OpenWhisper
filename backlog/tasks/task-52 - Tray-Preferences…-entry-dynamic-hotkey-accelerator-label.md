---
id: TASK-52
title: Tray Preferences… entry + dynamic hotkey accelerator label
status: To Do
assignee: []
created_date: '2026-04-27 15:29'
updated_date: '2026-04-27 15:29'
labels:
  - ui
  - tauri
  - tray
  - settings
dependencies:
  - TASK-49
  - TASK-51
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Wire the tray menu's Preferences… item now that Settings exists, and surface the configured hotkey as an accelerator hint next to Start/Stop Dictation. Depends on Settings shell + Shortcuts rebind.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Preferences… menu item with ⌘, accelerator opens the Settings window
- [ ] #2 Start/Stop Dictation row shows the configured hotkey label (e.g. 'Right ⌘', 'Shift+Space') — reads from the same settings store as the rebind pane
- [ ] #3 Label updates live when the user rebinds (no restart needed)
<!-- AC:END -->
