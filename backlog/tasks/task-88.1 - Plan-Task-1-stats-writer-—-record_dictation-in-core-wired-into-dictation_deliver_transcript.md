---
id: TASK-88.1
title: >-
  Plan Task 1: stats writer — record_dictation in core wired into
  dictation_deliver_transcript
status: In Progress
assignee:
  - '@claude'
created_date: '2026-05-06 06:14'
updated_date: '2026-05-06 08:42'
labels:
  - 88-impl
dependencies:
  - TASK-87.4
parent_task_id: TASK-88
ordinal: 53000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 core::stats::record_dictation exists with the spec signature; empty-text returns without inserting
- [ ] #2 core::stats::set_store(Arc<Store>) exists; idempotent first-call-wins like INJECTOR
- [ ] #3 dictation state gains record_start_epoch_ms field; dictation_deliver_transcript captures started_at_ms + duration_ms and calls record_dictation after inj.inject
- [ ] #4 Cancel and empty-transcript paths reach NO insert (verified by unit test walking dictation_mark_capture_stopped(0) + dictation_deliver_transcript('') and asserting zero rows)
- [ ] #5 DB-write failure logs at warn! and does not transition phase to PHASE_ERROR (verified by injecting a closed-store handle)
<!-- AC:END -->
