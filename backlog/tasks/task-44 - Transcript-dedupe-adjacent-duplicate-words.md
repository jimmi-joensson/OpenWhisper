---
id: TASK-44
title: 'Transcript: dedupe adjacent duplicate words'
status: Done
assignee: []
created_date: '2026-04-26 20:54'
labels:
  - transcript
  - core
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Speakers stutter — "let's let's check", "boat boat fish fish". Added a fourth pass to core/src/transcript.rs::process between substitutions and whitespace normalization that collapses runs of the same word separated by whitespace only. Punctuation glued to either token ("really, really nice", "no. No problem") protects intentional repetition and sentence boundaries. Comparison is case-insensitive; first occurrence's casing wins. Both shells benefit (Mac via swift-bridge, Tauri direct call).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 dedupe_repeats collapses consecutive case-insensitive matches
- [ ] #2 Punctuation between tokens (comma, period) protects repetition
- [ ] #3 First occurrence casing preserved
- [ ] #4 Triple+ runs collapse to single
- [ ] #5 Unit tests green (5 new cases)
<!-- AC:END -->
