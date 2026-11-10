---
id: TASK-93.8
title: 'Plan Task 8: Spoken instructions - tell the message from an instruction'
status: In Review
assignee: []
created_date: '2026-11-10T10:57:00Z'
labels:
  - 93-impl
dependencies: []
parent_task_id: TASK-93
ordinal: 72000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Instructions about wording, form or language spoken in the dictation are carried out and left out of the text
- [ ] #2 Questions and tasks meant for someone else are still typed as said, never answered or carried out
- [ ] #3 A rewrite is only accepted when the model declares the instruction; undeclared rewrites are rejected
- [ ] #4 Dictionary terms reach the prompt with their known mis-hearings
- [ ] #5 Live eval covers scratch-that, translate on request, tone, bullets, instruction next to a task, new name mis-hearing
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Live eval (19 cases, warmed): Sonnet low 19/19 twice; Haiku low 15-16/19 over 5 runs (misses: Danish left uncleaned, bullet request ignored, garbled word guessed). Prompt wording chosen by a controlled comparison of 7 variants (see comment in core/src/refine/prompt.rs). Awaiting user QA by dictating.
<!-- SECTION:NOTES:END -->
