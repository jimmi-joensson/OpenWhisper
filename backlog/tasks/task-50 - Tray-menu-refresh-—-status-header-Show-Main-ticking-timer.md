---
id: TASK-50
title: Tray menu refresh — status header + Show Main + ticking timer
status: Won't Do
assignee: []
created_date: '2026-04-27 15:28'
updated_date: '2026-04-30 16:35'
labels:
  - ui
  - tauri
  - tray
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Upgrade the system tray menu to match the design (components.jsx MenubarMock). Native NSStatusItem/Win tray menu — accept the styling gap from the frosted-popover mock; native menus can't be custom-rendered. Independent of Settings; ships first.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Disabled status header item: '● Recording · MM:SS' (orange dot) or '○ Idle · ready' (muted). Unicode bullets, monospace via system
- [ ] #2 Show Main Window menu item with ⌘1 accelerator (CmdOrCtrl+1) — reuses existing open_main()
- [ ] #3 While PHASE_RECORDING, tray watcher rebuilds menu every ~1s so the elapsed timer ticks
- [ ] #4 Quit OpenWhisper retains ⌘Q accelerator (no regression)
- [ ] #5 Existing Start/Stop Dictation entry stays; label tracks phase as today
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Closed during 2026-04-30 backlog review as Won't Do. Post-v0.4.0 priorities reset; tray menu refresh will be re-planned from current state if/when revisited.
<!-- SECTION:FINAL_SUMMARY:END -->
