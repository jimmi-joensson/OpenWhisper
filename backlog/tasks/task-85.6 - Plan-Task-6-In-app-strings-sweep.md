---
id: TASK-85.6
title: 'Plan Task 6: In-app strings sweep'
status: To Do
assignee: []
created_date: '2026-05-04 16:36'
updated_date: '2026-05-04 16:40'
labels:
  - 85-impl
milestone: m-1
dependencies: []
parent_task_id: TASK-85
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Find every user-visible 'OpenWhisper' string in code and replace with <NEW_NAME>: tray menu, Pill, Settings UI, error toasts, dialog boxes, window titles, log subsystem id (com.openwhisper.OpenWhisper → com.<new>.<NEW_NAME>), env vars (OPENWHISPER_VERBOSE → <NEW>_VERBOSE etc.) with backwards-compat read of old names + deprecation log line for one minor release.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 rg 'OpenWhisper|openwhisper' apps/tauri/src/ apps/tauri/src-tauri/src/ core/src/ cli/src/ returns zero hits (excluding comments and historical references)
- [ ] #2 Tray menu, Pill, Settings, error toasts, window titles all show new name in Mac smoke build
- [ ] #3 Setting <NEW>_VERBOSE=1 enables verbose mode; setting old OPENWHISPER_VERBOSE=1 still works with deprecation warning
<!-- AC:END -->
