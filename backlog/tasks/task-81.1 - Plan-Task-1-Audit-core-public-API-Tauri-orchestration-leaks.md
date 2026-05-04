---
id: TASK-81.1
title: 'Plan Task 1: Audit core/ public API + Tauri orchestration leaks'
status: To Do
assignee: []
created_date: '2026-05-04 15:09'
labels:
  - 81-impl
dependencies: []
parent_task_id: TASK-81
milestone: m-1
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Read every pub in core/src/ and every #[tauri::command] in apps/tauri/src-tauri/src/. Produce two audit docs as the basis for Tasks 2 and 3.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Audit doc 1 (core public API) committed; lists every pub fn, struct, enum, trait grouped by capability (capture / dictation / transcribe / device-enum / settings / diagnostics)
- [ ] #2 Audit doc 2 (shell orchestration leaks) committed; every Tauri shell symbol classified P/O/M with line-number citations
- [ ] #3 Concrete extraction checklist named in audit doc 2 ready to feed Task 2
- [ ] #4 Reviewer confirmed coverage: every pub in core/src/ in audit 1; every >5-line function in apps/tauri/src-tauri/src/lib.rs in audit 2
<!-- AC:END -->
