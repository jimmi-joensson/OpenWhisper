---
id: TASK-10
title: Custom vocabulary / biasing
status: To Do
assignee: []
created_date: '2026-04-22 21:11'
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
