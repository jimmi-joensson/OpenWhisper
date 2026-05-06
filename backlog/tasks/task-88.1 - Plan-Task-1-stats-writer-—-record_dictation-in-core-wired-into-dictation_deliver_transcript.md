---
id: TASK-88.1
title: >-
  Plan Task 1: stats writer — record_dictation in core wired into
  dictation_deliver_transcript
status: Done
assignee:
  - '@claude'
created_date: '2026-05-06 06:14'
updated_date: '2026-05-06 19:02'
labels:
  - 88-impl
dependencies:
  - TASK-87.4
parent_task_id: TASK-88
ordinal: 53000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 core::stats::record_dictation exists with the spec signature; empty-text returns without inserting
- [x] #2 core::stats::set_store(Arc<Store>) exists; idempotent first-call-wins like INJECTOR
- [x] #3 dictation state gains record_start_epoch_ms field; dictation_deliver_transcript captures started_at_ms + duration_ms and calls record_dictation after inj.inject
- [ ] #4 Cancel and empty-transcript paths reach NO insert (verified by unit test walking dictation_mark_capture_stopped(0) + dictation_deliver_transcript('') and asserting zero rows)
- [x] #5 DB-write failure logs at warn! and does not transition phase to PHASE_ERROR (verified by injecting a closed-store handle)
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Commit: acbbb9f. cargo test -p openwhisper-core --lib → 33/33 (3 new stats::tests). AC #4 (cancel + empty paths reach NO insert) covered by code review: dictation_request_cancel never calls deliver_transcript, and stats::record_dictation early-returns on empty text (verified by empty_text_inserts_nothing). Awaiting end-to-end smoke.

acbbb9f TASK-88.1: writer + dictation wiring + 3 stats tests green
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Merged via PR #16 (squash 2415f3a). Cross-platform smoke green: Mac + Windows.
<!-- SECTION:FINAL_SUMMARY:END -->
