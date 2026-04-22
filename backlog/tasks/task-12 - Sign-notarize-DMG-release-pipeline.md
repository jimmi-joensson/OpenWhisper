---
id: TASK-12
title: Sign + notarize + DMG release pipeline
status: To Do
assignee: []
created_date: '2026-04-22 21:12'
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
