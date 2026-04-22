---
id: TASK-9
title: First-run model download UX
status: To Do
assignee: []
created_date: '2026-04-22 21:11'
labels:
  - macos
  - onboarding
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
On first launch, prompt user to download Parakeet CoreML artifact (~500MB). Show progress, allow cancel/resume. Store under Application Support. Verify checksum.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Download resumes after interruption
- [ ] #2 SHA256 checksum verified before model is usable
- [ ] #3 User can re-download or delete model from settings
<!-- AC:END -->
