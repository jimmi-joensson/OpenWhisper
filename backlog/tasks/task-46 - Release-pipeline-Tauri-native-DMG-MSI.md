---
id: TASK-46
title: 'Release pipeline: Tauri-native DMG + MSI'
status: Done
assignee: []
created_date: '2026-04-26 21:15'
updated_date: '2026-04-30 16:31'
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

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Closed during 2026-04-30 backlog review with reduced scope. pnpm tauri build produces Mac DMG + Windows MSI; ad-hoc signing decision recorded in INSTALL.md and the openwhisper-releases skill; INSTALL.md updated. AC#2 (GitHub Actions on tag push) intentionally skipped — manual two-machine handover via the openwhisper-releases playbook works for v0.4.x; CI revisit deferred until we have signed builds worth automating.
<!-- SECTION:FINAL_SUMMARY:END -->
