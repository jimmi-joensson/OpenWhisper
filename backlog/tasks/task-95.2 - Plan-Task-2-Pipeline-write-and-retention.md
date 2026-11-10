---
id: TASK-95.2
title: 'Plan Task 2: Pipeline write and retention'
status: In Review
assignee: []
created_date: '2026-11-10T14:40:00Z'
updated_date: '2026-11-10T15:28:00Z'
labels:
  - 95-impl
dependencies: []
parent_task_id: TASK-95
ordinal: 87000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 With history off the row is written exactly as before (text columns NULL)
- [x] #2 With history on the row holds final text, original text when Refine changed it, and the target app
- [x] #3 Retention runs on every write and at startup, also with history switched off
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
dictation_deliver_transcript appends the entry to its day file (history on), prunes, then writes the stats row as before (counters only). The frontmost app is read after the injector call and only with history on. AC 1 and 2 now read 'day file' for 'row'.
<!-- SECTION:NOTES:END -->
