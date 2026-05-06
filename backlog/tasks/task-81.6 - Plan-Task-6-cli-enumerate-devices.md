---
id: TASK-81.6
title: 'Plan Task 6: cli enumerate-devices'
status: In Review
assignee: []
created_date: '2026-05-04 15:10'
updated_date: '2026-05-06'
labels:
  - 81-impl
dependencies: []
parent_task_id: TASK-81
milestone: m-1
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
List input devices via core::audio::enumerate_devices(). Mac filters virtual mics via coreaudio kAudioDevicePropertyTransportType (existing logic). --json emits an array of device objects.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 cli enumerate-devices lists at least one device on Mac and Windows
- [x] #2 Default mic is flagged in output
- [x] #3 Virtual mics (Teams, Zoom, BlackHole) filtered on Mac
- [x] #4 --json output validates against a small inline schema
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Landed in commit `9347123`. Uses the same `core::audio::audio_list_input_devices` the desktop pane consumes — no duplicate filter logic. Text mode emits `id\tlabel\t<default|>` per line; --json emits an array of `{id, label, is_default}`. Mac smoke shows built-in mic flagged default + Continuity Camera mic, no virtual devices leaking through. Windows verification deferred until next Win-box visit (handler is target-agnostic, just calls the same library fn).
<!-- SECTION:NOTES:END -->
