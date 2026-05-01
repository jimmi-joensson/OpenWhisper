---
id: TASK-61.4
title: 'Plan Task 4: Windows MediaController implementation'
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
- [ ] #1 WindowsMediaController enumerates SMTC sessions and pauses every session reporting Playing
- [ ] #2 Endpoint volume fades down before pause-send and snaps back after pause
- [ ] #3 resume_now resumes every previously-paused session and ramps endpoint back to original over 200ms
- [ ] #4 Mute fallback engages when no Playing sessions exist
- [ ] #5 Per-session WinRT errors logged but do not bail the whole flow
- [ ] #6 Smoke matrix passes: Spotify, Edge/YouTube, multi-app, no-session, cancel-mid-recording, setting=off
<!-- AC:END -->
