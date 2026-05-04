---
id: TASK-63.4
title: 'Plan Task 4: LLGuidance constraint + edit-list schema + applier + validator'
status: To Do
assignee: []
created_date: '2026-04-30 22:26'
updated_date: '2026-05-04 08:03'
labels:
  - 63-impl
dependencies: []
parent_task_id: TASK-63
ordinal: 17000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 LLGuidance grammar (or GBNF fallback) constrains output to JSON delete-span schema
- [ ] #2 parse_edits, validate_edits, apply_edits exist in cleanup/edits.rs
- [ ] #3 Validator rejects overlap, out-of-bounds, >50% deletion, UTF-8-boundary violations
- [ ] #4 cleanup() fails closed: any validator error returns the unchanged input + warn log
- [ ] #5 Unit tests cover all validator paths plus the happy path
<!-- AC:END -->
