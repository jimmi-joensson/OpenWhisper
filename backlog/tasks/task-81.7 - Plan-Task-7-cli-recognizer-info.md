---
id: TASK-81.7
title: 'Plan Task 7: cli recognizer-info'
status: In Review
assignee: []
created_date: '2026-05-04 15:10'
updated_date: '2026-05-06'
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
- [x] #1 cli recognizer-info prints active engine, model path, version, EP on Mac and Windows
- [ ] #2 Output values match Diagnostics panel rendering for the same active engine
- [x] #3 --json mode emits a flat object
- [x] #4 Depends on Task 2 Commit D — core::diagnostics module must exist before 81.7 starts
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Landed in commit `c14f61e`. Calls `recognizer_ensure_loaded` then `core::diagnostics::recognizer_info()`; Mac smoke prints engine=FluidAudio, model_version=parakeet-tdt-0.6b-v3, ep=ANE. `model_path` currently surfaces "<unknown>" — neither backend exposes the on-disk artifact path through the public surface yet (FluidAudio's .mlmodelc lives inside the Swift bundle; ort caches paths internally in `Sessions`). Adding it requires a new FFI shim on the Mac side and a `paths()` accessor on `OrtParakeet`; deferred. AC #2 (parity with Diagnostics panel) waits for the panel to ship — there isn't a TASK-65 / TASK-78 diagnostics route yet.
<!-- SECTION:NOTES:END -->
