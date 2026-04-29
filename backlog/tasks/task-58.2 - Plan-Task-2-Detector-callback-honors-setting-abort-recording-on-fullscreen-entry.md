---
id: TASK-58.2
title: >-
  Plan Task 2: Detector callback honors setting; abort recording on
  fullscreen-entry
status: Done
assignee:
  - '@claude'
created_date: '2026-04-29 18:05'
updated_date: '2026-04-29 20:49'
labels:
  - 58-impl
dependencies: []
parent_task_id: TASK-58
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Fullscreen callback reads behavior::show_in_fullscreen() and short-circuits when true
- [x] #2 Mid-recording fullscreen entry with setting=false aborts the recording silently — no transcript, no paste, returns to idle
- [x] #3 Toggling the setting while fullscreen is currently active immediately reconciles pill + hotkey state without restarting OW
- [x] #4 cargo check clean; manual smoke passes for the four cases (off+idle, off+recording, on+idle, toggle-on-during-fullscreen)
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
0a474e5 detector callback + listener via apply_fullscreen_state; reused do_cancel for abort path (no new core API); cargo check clean. AC#4 manual smoke deferred to end-of-TASK-58 DoD pass.

fad1364 defer pill.hide ~120ms after cancel so React renders IDLE before NSWindow orderOut caches the frame; eliminates orange-flash on fullscreen exit. AC#4 manual smoke verified end-to-end (off+idle, off+recording-with-clean-exit, on+idle, toggle-during-fullscreen).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Fullscreen detector callback honors behavior::show_in_fullscreen, aborts mid-recording silently when suppressing, and reconciles state on toggle without restart. Deferred pill hide eliminates the orange-flash artifact on fullscreen exit.
<!-- SECTION:FINAL_SUMMARY:END -->
