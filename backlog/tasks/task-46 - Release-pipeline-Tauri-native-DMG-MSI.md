---
id: TASK-46
title: 'Release pipeline: Tauri-native DMG + MSI'
status: To Do
assignee: []
created_date: '2026-04-26 21:15'
labels:
  - release
  - tauri
  - ci
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-41 archive: SwiftUI shell moved to archive/macos and the SwiftUI build path (scripts/{bootstrap,build-core,dev-run,reset-tcc,package-release}.sh + .github/workflows/release.yml) deleted because they targeted the retired Xcode project. The repo now ships exactly zero release artifacts. Need a Tauri-native equivalent: pnpm tauri build produces a Mac .app + .dmg and a Windows .msi; codesigning + notarization story; GitHub Actions workflow on tag push covering both OS runners; signed/ad-hoc decision documented. Block on TASK-41 parity completion before cutting v0.1.0.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 pnpm tauri build path documented for Mac (.app + .dmg) and Windows (.msi)
- [ ] #2 GitHub Actions workflow runs on tag push, builds both platforms, uploads draft release
- [ ] #3 Codesigning + notarization decision recorded (ad-hoc vs Developer ID vs notarized)
- [ ] #4 INSTALL.md updated for Tauri install flow
<!-- AC:END -->
