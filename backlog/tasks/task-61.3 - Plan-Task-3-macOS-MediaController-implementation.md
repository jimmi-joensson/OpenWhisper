---
id: TASK-61.3
title: 'Plan Task 3: macOS MediaController implementation'
status: To Do
assignee: []
created_date: '2026-04-30 22:18'
labels:
  - 61-impl
dependencies: []
parent_task_id: TASK-61
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 MacMediaController resolves MediaRemote via dlopen/dlsym; falls back to mute-only when symbols absent
- [ ] #2 Pause flow ramps endpoint to 0, sends kMRPause, snaps endpoint back to original
- [ ] #3 Resume flow snaps endpoint to 0, sends kMRPlay, ramps endpoint up to original over 200ms
- [ ] #4 Mute fallback ramps to 0 on pause and back to original on resume
- [ ] #5 State protected by mutex; rapid toggle does not queue overlapping fades
- [ ] #6 Smoke matrix passes: Spotify, Safari/YouTube, no-media-app, cancel-mid-recording, setting=off
<!-- AC:END -->
