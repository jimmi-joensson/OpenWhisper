---
id: TASK-95.3
title: 'Plan Task 3: openwhisper history commands'
status: In Review
assignee: []
created_date: '2026-11-10T14:40:00Z'
updated_date: '2026-11-10T15:28:00Z'
labels:
  - 95-impl
dependencies: []
parent_task_id: TASK-95
ordinal: 88000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 list, search, show, delete, clear and config work against the app's database and settings
- [x] #2 clear needs an explicit confirmation flag
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
openwhisper history list|search|show|delete|clear --yes|config (--enabled, --retention-days, --folder, --default-folder). Ids are text (20261110T140212). Run against the Dev app's folder with 23 real entries: list, search, config; rg over the folder finds text directly.
<!-- SECTION:NOTES:END -->
