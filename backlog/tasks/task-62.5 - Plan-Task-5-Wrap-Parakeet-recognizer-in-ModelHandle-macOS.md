---
id: TASK-62.5
title: 'Plan Task 5: Wrap Parakeet recognizer in ModelHandle (macOS)'
status: To Do
assignee: []
created_date: '2026-04-30 22:25'
labels:
  - 62-impl
dependencies: []
parent_task_id: TASK-62
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 ENGINE is a ModelHandle<Box<dyn Recognizer>> with 5-min idle timeout
- [ ] #2 recognizer_ensure_loaded and recognizer_transcribe retain their existing public signatures
- [ ] #3 FluidAudioBridge releases its Swift handle on Drop (verified or added)
- [ ] #4 After 5+ min idle, next dictation re-enters PHASE_LOADING_MODEL and succeeds
- [ ] #5 cargo check clean on aarch64-apple-darwin
<!-- AC:END -->
