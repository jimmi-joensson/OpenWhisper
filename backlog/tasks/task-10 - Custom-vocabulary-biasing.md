---
id: TASK-10
title: Custom vocabulary / biasing
status: Won't Do
assignee: []
created_date: '2026-04-22 21:11'
updated_date: '2026-04-30 16:35'
labels:
  - core
  - accuracy
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Let users add custom words (names, acronyms, jargon). Apply via post-processing: fuzzy-match low-confidence tokens against vocab, substitute. Parakeet doesn't support hot-word biasing natively, so this is a post-pass.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 User adds/removes vocab entries in settings
- [ ] #2 Vocab substitution fires on transcription output
- [ ] #3 Confidence threshold configurable
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Closed during 2026-04-30 backlog review as Won't Do. Post-v0.4.0 priorities reset; will be re-planned from current state if/when revisited. Parakeet quirks captured in the openwhisper-parakeet-quirks skill remain the forcing-function reference.
<!-- SECTION:FINAL_SUMMARY:END -->
