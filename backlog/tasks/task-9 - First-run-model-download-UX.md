---
id: TASK-9
title: First-run model download UX
status: Won't Do
assignee: []
created_date: '2026-04-22 21:11'
updated_date: '2026-04-30 16:35'
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

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Closed during 2026-04-30 backlog review as Won't Do. Post-v0.4.0 priorities reset — only TASK-60 (autostart backend) remains active from the prior To Do list. First-run model download UX will be re-planned from current state if/when revisited; v0.4.0 already ships a download-progress UI via core::dictation download_bytes_done/total.
<!-- SECTION:FINAL_SUMMARY:END -->
