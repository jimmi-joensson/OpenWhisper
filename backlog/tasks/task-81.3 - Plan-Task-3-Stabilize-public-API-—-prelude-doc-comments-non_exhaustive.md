---
id: TASK-81.3
title: 'Plan Task 3: Stabilize public API — prelude, doc-comments, non_exhaustive'
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
Treat the post-extraction public surface as a real API: prelude module, doc-comments on every public item in prelude scope, #[non_exhaustive] discipline on enums and future-extensible structs.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 core::prelude module exists and exports the canonical types named in the spec
- [ ] #2 Every pub item in prelude-exported modules has a doc-comment explaining return type and failure modes
- [ ] #3 cargo doc --no-deps -p openwhisper-core renders without warnings on prelude items
- [ ] #4 cargo build --workspace clean
- [ ] #5 Every named pub enum + struct (Phase, Toggle, SelectedDeviceStatus at audio.rs:609, TranscribeResult at recognizer/mod.rs:44, Snapshot, RecognizerInfo, DiagnosticsReadout, CrashDump) is #[non_exhaustive] or has comment citing concrete reason
- [ ] #6 cargo build -p openwhisper-core --features macos-shell still clean — prelude reshuffle does not break SwiftUI shell
- [ ] #7 core/src/prelude.rs header comment documents that prelude is for default + tauri features; macos-shell uses per-module FFI signatures
<!-- AC:END -->
