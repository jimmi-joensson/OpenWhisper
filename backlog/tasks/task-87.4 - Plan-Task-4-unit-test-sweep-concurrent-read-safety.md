---
id: TASK-87.4
title: 'Plan Task 4: unit test sweep + concurrent-read safety'
status: In Progress
assignee:
  - '@claude'
created_date: '2026-05-06 06:10'
updated_date: '2026-05-06 10:19'
labels:
  - 87-impl
dependencies:
  - TASK-87.2
  - TASK-87.3
parent_task_id: TASK-87
ordinal: 51000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 cargo test -p openwhisper-core store:: includes ≥4 tests covering: file+parent-dir creation, open-twice idempotency, INSERT/reopen/SELECT round-trip, concurrent reads from 8 threads × 100 iterations
- [ ] #2 Concurrent-read test asserts no panics, no deadlock, consistent counts; wrapped with a 5-second test timeout
- [ ] #3 Tests actually executed (not just compiled), pass on macOS and Windows
<!-- AC:END -->
