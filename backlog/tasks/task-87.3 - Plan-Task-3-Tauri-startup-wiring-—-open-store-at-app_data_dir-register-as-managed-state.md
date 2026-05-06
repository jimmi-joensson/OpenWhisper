---
id: TASK-87.3
title: >-
  Plan Task 3: Tauri startup wiring — open store at app_data_dir, register as
  managed state
status: In Review
assignee:
  - '@claude'
created_date: '2026-05-06 06:09'
updated_date: '2026-05-06 08:42'
labels:
  - 87-impl
dependencies:
  - TASK-87.1
parent_task_id: TASK-87
ordinal: 50000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Tauri setup hook calls Store::open_or_init(app_data_dir.join(openwhisper.db)) and registers via app.manage(store)
- [ ] #2 App launches successfully on macOS and Windows; openwhisper.db file appears at the expected per-platform path on first launch
- [x] #3 Path-resolution / unwritable-path failure does NOT panic; app continues, error logged via tracing::error!, dictation flow unaffected
- [ ] #4 app.state::<Store>() resolves inside any Tauri command after setup completes (verified by a throwaway probe command added in Task 4)
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Commit: 5378a40. cargo check -p openwhisper-tauri clean. AC #2 (file appears on launch) + AC #4 (state resolves in cmd) deferred to manual smoke at end. Logging convention delta documented in commit body.

5378a40 TASK-87.3: open store in setup, app.manage
<!-- SECTION:NOTES:END -->
