---
id: TASK-81.2
title: 'Plan Task 2: Extract orchestration from Tauri shell into core/'
status: To Do
assignee: []
created_date: '2026-05-04 15:09'
updated_date: '2026-05-04 15:16'
labels:
  - 81-impl
milestone: m-1
dependencies: []
parent_task_id: TASK-81
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
For each (O) and (M) symbol from Task 1's audit: move orchestration into core/, leave platform glue in shell. Likely candidates: media-pause/resume gate, settings store, behavior gating logic.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Every (O) symbol from audit doc 2 lives in core/
- [ ] #2 Every (M) symbol split: orchestration in core, platform glue in shell
- [ ] #3 cargo check --workspace clean; cargo build -p openwhisper-core --features tauri clean
- [ ] #4 pnpm test:ui green from apps/tauri/; no behavior regression in dictation flow
- [ ] #5 core::diagnostics module exists in core/ with RecognizerInfo, DiagnosticsReadout, placeholder CrashDumpReader trait + CrashDump struct
- [ ] #6 cargo build -p openwhisper-core --features macos-shell stays clean (SwiftUI shell isn't broken by the extraction)
- [ ] #7 apps/tauri/src-tauri/src/lib.rs reduction is explainable by audit doc 2's extraction list — no hard line target
<!-- AC:END -->
