---
id: TASK-54
title: 'Settings — General: Open at Login + tray checkbox'
status: To Do
assignee: []
created_date: '2026-04-27 15:29'
updated_date: '2026-04-29 08:26'
labels:
  - ui
  - tauri
  - settings
dependencies:
  - TASK-56
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add tauri-plugin-autostart and surface the toggle in two places: Settings → General (per design SettingsGeneralBoard) and the tray menu's Open at Login row (✓ when enabled). Single source of truth = autostart plugin state.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 tauri-plugin-autostart wired and registered
- [ ] #2 Settings → General has Launch at login toggle; reads/writes via the plugin
- [ ] #3 Tray menu 'Open at Login' uses CheckMenuItemBuilder; ✓ reflects current state and toggling updates both surfaces
- [ ] #4 Theme picker (System/Light/Dark) is a stub — no behavior change yet, just renders per design (no-op until needed)
<!-- AC:END -->
