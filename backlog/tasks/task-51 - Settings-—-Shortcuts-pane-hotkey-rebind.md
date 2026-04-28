---
id: TASK-51
title: Settings — Shortcuts pane + hotkey rebind
status: To Do
assignee: []
created_date: '2026-04-27 15:29'
updated_date: '2026-04-27 15:29'
labels:
  - ui
  - tauri
  - hotkey
  - settings
dependencies:
  - TASK-49
references:
  - 'https://github.com/jimmi-joensson/OpenWhisper/issues/5'
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement Settings → Shortcuts pane per design (screens.jsx SettingsShortcutsBoard). Capture-on-click rebind for the global toggle hotkey, persisted across launches. Closes GitHub issue #5 (Ctrl+Space conflicts in browsers). Default per platform stays as today: Right ⌘ on macOS, Ctrl+Space on Windows. Esc-to-cancel stays fixed.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Hotkey persists to a settings file (tauri-plugin-store or hand-rolled JSON in app data dir)
- [x] #2 Capture flow: click button → 'press keys…' state → record next chord/modifier-tap → save + restart hotkey install. Cancel button restores previous binding
- [x] #3 Reset to default link restores the platform default
- [x] #4 macOS hotkey backend supports both modifier-tap (Right ⌘ today) AND chord (e.g. Shift+Space). Two execution paths in mac.rs handle_event — modifier-tap stays, chord branches off KeyDown with modifier mask
- [x] #5 Windows hotkey backend parameterizes vk + modifiers (already chord-only, just wire the config)
- [x] #6 Captured chord descriptor is cross-platform JSON: { kind: 'modifier-tap'|'chord', code: string, mods: string[] }
- [x] #7 HotkeyChip + 'press keys…' UI matches design components
<!-- AC:END -->
