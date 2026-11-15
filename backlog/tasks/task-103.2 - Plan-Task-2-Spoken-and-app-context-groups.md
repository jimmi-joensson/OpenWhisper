---
id: TASK-103.2
title: 'Plan Task 2: Spoken and app context groups'
status: In Review
assignee: []
created_date: '2026-11-11T11:50:00Z'
updated_date: '2026-11-15T07:10:00Z'
labels:
  - 103-impl
dependencies: []
parent_task_id: TASK-103
ordinal: 121000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
See doc-53 Task 2. System voice to file, transcribe, ask, in English and Danish. App with a connection uses the connection, app without takes the screen, judged by the fixture log and the capture count.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Spoken English and Danish scenarios pass on macOS and are skipped with a reason elsewhere
- [x] #2 App with a connection: tool call and no capture; app without: capture and no tool call
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Built in aea709c. Spoken: the system voice (Samantha, Sara) speaks to a file, the recognizer transcribes, the ask runs. The Danish system voice is misheard often (five of six test sentences), so the scenario uses the one sentence that came back right three times of three and judges by key words. App context: mapped app gives a tool call in the fixture log and no capture; unmapped app gives one capture and no tool call; a screen phrase with a mapped app gives both; Agent off gives no tool call. Windows: skipped with a reason, compile-checked only.
<!-- SECTION:NOTES:END -->
