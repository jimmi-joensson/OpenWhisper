---
id: TASK-12
title: Sign + notarize + DMG release pipeline
status: Won't Do
assignee: []
created_date: '2026-04-22 21:12'
updated_date: '2026-04-30 16:32'
labels:
  - macos
  - release
dependencies: []
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
GitHub Actions workflow: build, sign with Developer ID, notarize via notarytool, package DMG, publish to Releases. Separate workflow for unsigned dev builds.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Tagged releases produce signed + notarized DMG
- [ ] #2 Sparkle or similar autoupdate wired up (optional for MVP)
- [ ] #3 Secrets for cert + notary stored in Actions secrets
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Closed during 2026-04-30 backlog review as Won't Do. Mac-only Xcode/Developer-ID/notarytool path is dead — apps/macos retired to archive/macos/, Tauri is the sole shell. Tauri-native equivalent tracked under TASK-46 (also closed; ad-hoc signed for v0.4.x). Re-open if/when we move to paid Developer ID + notarization.
<!-- SECTION:FINAL_SUMMARY:END -->
