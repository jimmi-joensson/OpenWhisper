---
id: TASK-41
title: Tauri Mac parity audit + apps/macos retirement
status: To Do
assignee: []
created_date: '2026-04-26 18:49'
labels:
  - tauri
  - phase-7
  - parity
  - cleanup
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Phase 7 leftover: close visual + behavioral parity gaps between the Tauri Mac build and the shipped SwiftUI app under apps/macos/, then retire apps/macos/.

Known gaps (pill not fully ported per user, possibly other macOS-only behaviors). Audit method: read apps/macos/App/*.swift side-by-side with apps/tauri/, enumerate behavioral / visual deltas, fix or backlog each. apps/macos/ is removed only after Tauri ships as the Mac release per project_tauri_port decision.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Punch list of Tauri vs apps/macos/ deltas captured (pill, main window, tray, menu copy, permissions flow)
- [ ] #2 All non-deferred deltas closed in Tauri
- [ ] #3 Tauri ships as the Mac release (replaces shipped SwiftUI build)
- [ ] #4 apps/macos/ removed from repo
- [ ] #5 README.md + INSTALL.md updated for single-Tauri-app repo
<!-- AC:END -->
