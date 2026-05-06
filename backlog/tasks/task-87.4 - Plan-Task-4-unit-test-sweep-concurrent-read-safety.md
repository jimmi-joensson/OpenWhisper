---
id: TASK-87.4
title: 'Plan Task 4: unit test sweep + concurrent-read safety'
status: In Review
assignee:
  - '@claude'
created_date: '2026-05-06 06:10'
updated_date: '2026-05-06 10:20'
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
- [x] #1 cargo test -p openwhisper-core store:: includes ≥4 tests covering: file+parent-dir creation, open-twice idempotency, INSERT/reopen/SELECT round-trip, concurrent reads from 8 threads × 100 iterations
- [x] #2 Concurrent-read test asserts no panics, no deadlock, consistent counts; wrapped with a 5-second test timeout
- [ ] #3 Tests actually executed (not just compiled), pass on macOS and Windows
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Commit: a5bd240. cargo test -p openwhisper-core --lib store:: → 6/6. AC #3 (Win run) deferred — no Win box in loop, would land via CI.

a5bd240 TASK-87.4: 8x100 concurrent-read sweep + 5s ceiling
<!-- SECTION:NOTES:END -->
