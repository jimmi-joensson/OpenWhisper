---
id: TASK-99.4
title: 'Plan Task 4: Tests and verification'
status: In Review
assignee: []
created_date: '2026-11-11T06:38:00Z'
updated_date: '2026-11-15T06:53:00Z'
labels:
  - 99-impl
dependencies: []
parent_task_id: TASK-99
ordinal: 114000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Core, CLI and shell cargo tests, Windows target check and Playwright pass
- [ ] #2 Spike result, eval numbers, run times and the terms note are in the TASK-99 notes
- [ ] #3 TASK-99 is In Review with the unverified items listed
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Orchestrator's own runs on 25f66f9 (2026-11-15): core lib 875, CLI 84, shell 96+1, vitest 40, Playwright 440 of 440, Windows target checks clean (core, CLI, shell). agent eval --offline 122 of 122. Live eval (Sonnet, low effort, local fixture only) 18 of 18, no send or delete reached the fixture where none was expected: read 3.4 s median, draft 5.6 s, confirmed send 3.1 s. Two independent reviews (core and CLI; shell and UI) on the diff against task-98-vision; every confirmed finding fixed in 8b9cf57 and 25f66f9.
<!-- SECTION:NOTES:END -->
