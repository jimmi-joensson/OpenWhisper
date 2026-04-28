---
id: TASK-11
title: Settings window
status: Won't Do
assignee: []
created_date: '2026-04-22 21:11'
updated_date: '2026-04-27 18:30'
labels:
  - macos
  - ui
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Superseded by TASK-49 (Settings shell) + TASK-51 (Shortcuts pane + rebind) + TASK-53 (Audio pane) + TASK-54 (General: Open at Login). Original SwiftUI scope no longer applies — Tauri is the sole shell post-port. Vocabulary tab folded into TASK-10. Models tab tracked in TASK-45. Attribution lives in INSTALL.md / About panel (TASK-13).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 All settings persist to UserDefaults or config file
- [ ] #2 Attribution tab shows NVIDIA Parakeet CC-BY-4.0 notice per license
- [ ] #3 Launch-at-login toggle uses SMAppService
<!-- AC:END -->
