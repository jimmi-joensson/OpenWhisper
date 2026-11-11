---
id: TASK-98.1
title: >-
  Plan Task 1: Core settings, trigger, macOS capture, paint layer, ask
  integration
status: In Review
assignee: []
created_date: '2026-11-10T23:24:00Z'
updated_date: '2026-11-11T06:37:00Z'
labels:
  - 98-impl
dependencies: []
parent_task_id: TASK-98
ordinal: 106000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Phrase matching is local and tested in English and Danish; off never captures; smart without a phrase makes a text-only first turn and captures only on the marker (stub tests)
- [x] #2 capture without an ask in progress is an error and the dictation path cannot reach it (test)
- [x] #3 Frontmost app on the list means no capture and a notice; a failed exclusion or window listing means no capture (tests with a fake platform layer)
- [x] #4 Paint layer blacks out the given rectangles in a synthetic image at every scale, also after the downscale (pixel tests)
- [x] #5 A capture ends as PNG bytes in memory, the backends receive them, screen=yes is stored on the history entry and old files parse as before
- [x] #6 Live env-gated macOS test with a solid-colour test window: not excluded it is in the image, excluded it is not, paint-only it is black
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
dd84596 core vision. 583 core lib tests (499 before), run by the orchestrator; default-feature and Windows target checks clean. New dependency: objc2-screen-capture-kit 0.3 (Zlib OR Apache-2.0 OR MIT); png, block2, objc2-core-graphics were already in the lockfile. The downscale is own code. Screen Recording permission for the terminal reads as granted. The size-only live capture test passed with the display on (it returns quietly when there is no display, so it proves little). AC 6, the live pixel test with a test window, comes with the CLI (98.3).

Live pixel test run by the orchestrator on 2026-11-11 with the display on (OPENWHISPER_VISION_LIVE=1 cargo test -p openwhisper-cli --test vision_live -- --ignored --nocapture): control magenta (255,0,255) at 5 points; excluded: green (0,200,0), the window behind shows; painted: black; excluded and painted: black; the green window beside it unchanged in every case; control again magenta. Image 1568x656, capture 163 to 185 ms.
<!-- SECTION:NOTES:END -->
