---
id: TASK-38
title: Tauri Phase 7 — Polish pass + archive WinUI 3 + retire apps/macos
status: To Do
assignee: []
created_date: '2026-04-24 22:07'
labels:
  - tauri
  - phase-7
  - polish
  - cleanup
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Ship criteria: Tauri build at feature + visual parity with the shipped Mac SwiftUI app on both OSes.

1. Side-by-side compare with apps/macos/ on Mac. Close remaining visual/behavioral gaps.
2. RDP + multi-monitor sanity check on Windows.
3. Fullscreen behavior verified across Chromium, video apps, games.
4. Archive apps/windows/ platform tricks to backlog/decisions/ with code inline before deleting: fullscreen detect (PillWindow.xaml.cs), EscapeHook.cs, TextInjector.cs paste dance, StatusIconRenderer.cs tray renderer, settings JSON handling.
5. Delete apps/windows/ after archive.
6. Retire apps/macos/ — once Tauri is shipped as the Mac release, remove apps/macos/ (the Mac SwiftUI app is replaced, not maintained in parallel per the Tauri port decision).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Tauri Mac build visually matches apps/macos/ SwiftUI app (pill, main window, tray, menu wording)
- [ ] #2 Tauri Windows build visually matches Mac Tauri build
- [ ] #3 Fullscreen behavior verified across Chromium fullscreen, a video app, a fullscreen game on each OS
- [ ] #4 RDP + multi-monitor sanity check passes on Windows
- [ ] #5 backlog/decisions/ has records for: Windows fullscreen detect, EscapeHook, paste dance, tray renderer, settings JSON — with code inline
- [ ] #6 apps/windows/ removed from repo
- [ ] #7 apps/macos/ removed from repo after Tauri Mac release ships
- [ ] #8 README.md updated to reflect single Tauri app; INSTALL.md updated for new build flow
<!-- AC:END -->
