---
id: TASK-86.1
title: 'Plan Task 1: recognizer_info Tauri cmd + trait method'
status: To Do
assignee: []
created_date: '2026-05-06 05:12'
updated_date: '2026-05-06 05:17'
labels:
  - 86-impl
dependencies: []
parent_task_id: TASK-86
ordinal: 43000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 RecognizerInfo struct exists in core::recognizer with name + origin String fields, derives Serialize + Clone
- [ ] #2 FluidAudioBridge and OrtParakeet both implement info() returning concrete strings (Parakeet, on-device)
- [ ] #3 recognizer_info() core accessor returns Some(RecognizerInfo) when the engine is initialized and None before recognizer_ensure_loaded has run
- [ ] #4 recognizer_info Tauri command callable from React via invoke<RecognizerInfo | null>('recognizer_info'), registered in invoke_handler! macro
- [ ] #5 Unit tests cover each info() impl asserting non-empty name + origin, plus the accessor returning None pre-load and Some post-load
<!-- AC:END -->
