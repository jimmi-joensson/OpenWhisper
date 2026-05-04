---
id: TASK-81.7
title: 'Plan Task 7: cli recognizer-info'
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
Print RecognizerInfo { engine, model_path, version, ep } via core::diagnostics::recognizer_info(). Should match what the Tauri Diagnostics panel shows.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 cli recognizer-info prints active engine, model path, version, EP on Mac and Windows
- [ ] #2 Output values match Diagnostics panel rendering for the same active engine
- [ ] #3 --json mode emits a flat object
- [ ] #4 Depends on Task 2 Commit D — core::diagnostics module must exist before 81.7 starts
<!-- AC:END -->
