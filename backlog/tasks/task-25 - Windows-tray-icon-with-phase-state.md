---
id: TASK-25
title: Windows tray icon with phase state
status: In Review
updated_date: '2026-04-24 20:30'
assignee: []
created_date: '2026-04-24 18:45'
labels:
  - windows
  - ui
dependencies:
  - TASK-23
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Windows equivalent of the macOS menubar status icon. Tray icon flips between idle (template/mono) and recording (orange `#E07000`) states, matching the Mac behavior where the menubar icon is the user's primary "is OpenWhisper armed?" signal. Right-click opens a menu with phase-aware labels (covered in TASK-30). Double-click opens the main window.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Tray icon visible in notification area after app launch
- [ ] #2 Idle state: monochrome/template icon that respects system theme (dark/light)
- [ ] #3 Recording state: orange `#E07000` composite icon
- [ ] #4 Icon swaps reactively on Rust-core phase changes (polled snapshot, not duplicated state)
- [ ] #5 Double-click opens / focuses main window
- [ ] #6 Right-click surfaces a menu (menu contents covered by TASK-30)
- [ ] #7 Tooltip shows current phase ("OpenWhisper — idle" / "OpenWhisper — recording" / …)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Evaluate `H.NotifyIcon.WinUI` vs direct `Shell_NotifyIcon` P/Invoke. Prefer library unless it pulls heavy deps. 2. Ship two ICO assets: idle (template-style for system-theme tinting) and recording (pre-rendered orange). 3. Hook tray icon creation to `App.OnLaunched`. 4. Subscribe to DictationService phase polling; swap icon + tooltip on change. 5. Wire double-click → existing `MainWindow` activation path. 6. Leave right-click menu as a stub until TASK-30.
<!-- SECTION:PLAN:END -->
