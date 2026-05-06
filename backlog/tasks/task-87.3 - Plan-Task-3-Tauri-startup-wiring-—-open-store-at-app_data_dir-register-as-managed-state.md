---
id: TASK-87.3
title: >-
  Plan Task 3: Tauri startup wiring — open store at app_data_dir, register as
  managed state
status: To Do
assignee: []
created_date: '2026-05-06 06:09'
labels:
  - 87-impl
dependencies:
  - TASK-87.1
parent_task_id: TASK-87
ordinal: 50000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Tauri setup hook calls Store::open_or_init(app_data_dir.join(openwhisper.db)) and registers via app.manage(store)
- [ ] #2 App launches successfully on macOS and Windows; openwhisper.db file appears at the expected per-platform path on first launch
- [ ] #3 Path-resolution / unwritable-path failure does NOT panic; app continues, error logged via tracing::error!, dictation flow unaffected
- [ ] #4 app.state::<Store>() resolves inside any Tauri command after setup completes (verified by a throwaway probe command added in Task 4)
<!-- AC:END -->
