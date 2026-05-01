---
id: TASK-65.6
title: 'Plan Task 6: TranscriptRow + wire into HomePane'
status: Done
assignee: []
created_date: '2026-04-30 22:45'
updated_date: '2026-05-01 14:22'
labels:
  - 65-impl
dependencies: []
parent_task_id: TASK-65
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 <TranscriptRow> renders text + relative timestamp ('just now' / '2m ago') + copy button.
- [ ] #2 Copy button is opacity:0 until row hover or button focus-visible; click writes text to clipboard and briefly shows a check icon.
- [ ] #3 HomePane renders the row beneath the hero only when useLastTranscription() returns non-null.
- [ ] #4 home.spec.ts new tests pass: row appears after finalization, row replaces (not appends), hover reveals copy button, copy writes to clipboard, 'just now' renders for fresh transcripts.
- [ ] #5 pnpm tsc --noEmit clean.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Commit: 8c9c29e — TranscriptRow + wire HomePane. 54/54 Playwright; tsc clean.
<!-- SECTION:NOTES:END -->
