---
id: TASK-63.1
title: 'Plan Task 1: llama-cpp-2 dependency + GGUF infrastructure'
status: To Do
assignee: []
created_date: '2026-04-30 22:26'
updated_date: '2026-05-04 08:03'
labels:
  - 63-impl
dependencies: []
parent_task_id: TASK-63
ordinal: 14000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 llama-cpp-2 is a core dependency with Metal feature on Mac, Vulkan on Windows
- [ ] #2 cleanup::paths::cleanup_models_dir() returns the platform-correct directory
- [ ] #3 cleanup::paths::gguf_path(variant) returns the right path for qwen3.5-0.8b-q4 and qwen3.5-2b-q4
- [ ] #4 cargo check clean on both aarch64-apple-darwin and x86_64-pc-windows-msvc
<!-- AC:END -->
