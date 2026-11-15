---
id: TASK-103.1
title: 'Plan Task 1: Scenario runner; everyday, modes and failure groups'
status: In Review
assignee: []
created_date: '2026-11-11T11:50:00Z'
updated_date: '2026-11-15T07:10:00Z'
labels:
  - 103-impl
dependencies: []
parent_task_id: TASK-103
ordinal: 120000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
See doc-53 Task 1. CLI openwhisper scenarios list and run, scratch config folder only, offline self-test, the everyday, modes and failures groups.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 scenarios list and run exist with --only, --json and --offline
- [x] #2 The runner refuses a real config folder
- [x] #3 Offline self-test passes for these groups
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Built in aea709c, extended in d9ba49c. openwhisper scenarios list and run with --only, --json, --offline and --config-dir. Scratch config folder for every run; a real config folder (or one inside it, or reached through a link) is refused. Offline self-test: 325 of 325 checks.
<!-- SECTION:NOTES:END -->
