---
id: TASK-62.1
title: 'Plan Task 1: Memory query primitive'
status: To Do
assignee: []
created_date: '2026-04-30 22:25'
updated_date: '2026-05-04 08:03'
labels:
  - 62-impl
dependencies: []
parent_task_id: TASK-62
ordinal: 4000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 ProcessMemory type defined with rss_bytes, peak_rss_bytes, timestamp
- [ ] #2 query_process_memory() returns non-zero RSS on the current process
- [ ] #3 Unit test covers RSS grows after allocation; peak >= current
- [ ] #4 cargo check and cargo test clean for openwhisper-core
<!-- AC:END -->
