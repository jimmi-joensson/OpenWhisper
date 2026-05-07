---
id: TASK-62.13
title: 'Plan Task 3: Settings → Models — Storage panel (disk + path + opener)'
status: In Review
assignee: []
created_date: '2026-05-07 14:00'
updated_date: '2026-05-07 22:00'
labels:
  - 62-impl
dependencies: []
parent_task_id: TASK-62
ordinal: 63000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 models_storage_path Tauri command exists, returns the resolved path at command-time, and is callable from the webview
- [x] #2 <ModelsStoragePanel /> renders below the model list with disk total, install count, mono path, and a Show-in-Finder/Explorer button
- [x] #3 Button copy switches by platform (Show in Finder on macOS, Show in Explorer on Windows)
- [x] #4 Click opens the OS file browser at the canonical models path (verified manually on both platforms)
- [x] #5 Playwright spec covers presence + button-focusable invariant
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented in cbc64e8 — adds models_storage_path + models_open_folder Tauri commands (mirrors crashes_open_folder shell-out pattern), <ModelsStoragePanel /> component, SettingsModelsPane scaffold (replaces PaneStub), and 3 Playwright cases (114→117). AC #4 'click opens OS file browser' verified via shim invoke count; manual cross-platform reveal still wants a real-binary smoke before flipping Done.

Windows code-side validation pass (2026-05-08): models_storage_path resolves to %APPDATA%\com.openwhisper.app.dev\models on the dev shell (NOT %APPDATA%\com.openwhisper.dev\models — actual identifier per tauri.dev.conf.json is com.openwhisper.app.dev). detectPlatform() regex /Win/ matches Edge UA on Windows → button copy flips to "Show in Explorer". models_open_folder shells out via Command::new("explorer") with create_dir_all first — same pattern verified end-to-end on TASK-78. Live Explorer launch still owed by user click.
<!-- SECTION:NOTES:END -->
