---
id: TASK-93.4
title: 'Plan Task 4: Wire Refine into the dictation pipeline'
status: In Review
assignee: []
created_date: '2026-11-10T09:14:00Z'
updated_date: '2026-11-10T09:48:00Z'
labels:
  - 93-impl
dependencies: []
parent_task_id: TASK-93
ordinal: 68000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 With Refine off, pipeline output is unchanged and no process or request starts
- [ ] #2 With Refine on, delivered text is refined and the snapshot carries raw_transcript, refine status and detail
- [ ] #3 Record start warms the backend; cancel and empty recordings discard it
- [x] #4 Status message reads refining while the call is in flight
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Refine stage in run_stop_pipeline, warm at record start, discard on cancel/empty/error, RefineInfo on the snapshot.
Unit-tested at the refine::run_for_dictation / dictation state level. ACs 2 and 3 are left unchecked on purpose: the glue in run_toggle / run_stop_pipeline needs the mic and has not been driven in the running app. Awaiting user QA by dictating.
<!-- SECTION:NOTES:END -->
