---
id: TASK-98.1
title: >-
  Plan Task 1: Core settings, trigger, macOS capture, paint layer, ask
  integration
status: To Do
assignee: []
created_date: '2026-11-10T23:24:00Z'
labels:
  - 98-impl
dependencies: []
parent_task_id: TASK-98
ordinal: 106000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Phrase matching is local and tested in English and Danish; off never captures; smart without a phrase makes a text-only first turn and captures only on the marker (stub tests)
- [ ] #2 capture without an ask in progress is an error and the dictation path cannot reach it (test)
- [ ] #3 Frontmost app on the list means no capture and a notice; a failed exclusion or window listing means no capture (tests with a fake platform layer)
- [ ] #4 Paint layer blacks out the given rectangles in a synthetic image at every scale, also after the downscale (pixel tests)
- [ ] #5 A capture ends as PNG bytes in memory, the backends receive them, screen=yes is stored on the history entry and old files parse as before
- [ ] #6 Live env-gated macOS test with a solid-colour test window: not excluded it is in the image, excluded it is not, paint-only it is black
<!-- AC:END -->
