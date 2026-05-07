---
id: TASK-62.13
title: 'Plan Task 3: Settings → Models — Storage panel (disk + path + opener)'
status: To Do
assignee: []
created_date: '2026-05-07 14:00'
labels:
  - 62-impl
dependencies: []
parent_task_id: TASK-62
ordinal: 63000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 models_storage_path Tauri command exists, returns the resolved path at command-time, and is callable from the webview
- [ ] #2 <ModelsStoragePanel /> renders below the model list with disk total, install count, mono path, and a Show-in-Finder/Explorer button
- [ ] #3 Button copy switches by platform (Show in Finder on macOS, Show in Explorer on Windows)
- [ ] #4 Click opens the OS file browser at the canonical models path (verified manually on both platforms)
- [ ] #5 Playwright spec covers presence + button-focusable invariant
<!-- AC:END -->
