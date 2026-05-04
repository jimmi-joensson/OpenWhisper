---
id: TASK-63.3
title: 'Plan Task 3: CleanupEngine trait + bare LlamaCpp implementation'
status: To Do
assignee: []
created_date: '2026-04-30 22:26'
updated_date: '2026-05-04 08:03'
labels:
  - 63-impl
dependencies: []
parent_task_id: TASK-63
ordinal: 16000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 CleanupEngine trait with ensure_loaded and cleanup defined
- [ ] #2 LlamaCppCleanup loads a GGUF via llama-cpp-2 and runs mmap + warmup forward pass
- [ ] #3 Bare cleanup() returns model output (unconstrained — flagged unsafe for production until Task 4)
- [ ] #4 File header explicitly warns about safety until Task 4
<!-- AC:END -->
