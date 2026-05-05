---
id: TASK-81.6
title: 'Plan Task 6: cli enumerate-devices'
status: To Do
assignee: []
created_date: '2026-05-04 15:10'
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
- [ ] #1 cli enumerate-devices lists at least one device on Mac and Windows
- [ ] #2 Default mic is flagged in output
- [ ] #3 Virtual mics (Teams, Zoom, BlackHole) filtered on Mac
- [ ] #4 --json output validates against a small inline schema
<!-- AC:END -->
