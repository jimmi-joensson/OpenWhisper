---
id: TASK-11
title: Settings window
status: To Do
assignee: []
created_date: '2026-04-22 21:11'
labels:
  - macos
  - ui
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
SwiftUI settings window with tabs: General (hotkey, pill position, launch-at-login), Models (picker, download/delete), Vocabulary, Advanced (VAD sensitivity, post-processing toggles), About (CC-BY-4.0 attribution).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 All settings persist to UserDefaults or config file
- [ ] #2 Attribution tab shows NVIDIA Parakeet CC-BY-4.0 notice per license
- [ ] #3 Launch-at-login toggle uses SMAppService
<!-- AC:END -->
