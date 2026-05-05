---
id: TASK-81.10
title: 'Plan Task 10: Refactor Tauri commands to one-liners over library API'
status: To Do
assignee: []
created_date: '2026-05-04 15:10'
updated_date: '2026-05-04 15:17'
labels:
  - 81-impl
milestone: m-1
dependencies: []
parent_task_id: TASK-81
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Walk every #[tauri::command] in apps/tauri/src-tauri/src/. Where command body has business logic, extract to core:: and reduce command to delegating call. Removes drift between CLI and UI shells.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Every #[tauri::command] body is <=5 lines or has clear platform-glue justification comment
- [ ] #2 pnpm test:ui from apps/tauri/ green
- [ ] #3 Manual dictation flow on Mac (record → transcribe → paste) passes
- [ ] #4 Any business logic discovered in shell that wasn't on audit doc 2's list is filed as follow-up subtask, not silently extracted (Task 10 is cleanup pass, not second extraction)
<!-- AC:END -->
