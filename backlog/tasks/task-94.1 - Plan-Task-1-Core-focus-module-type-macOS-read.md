---
id: TASK-94.1
title: 'Plan Task 1: Core focus module - type, macOS read'
status: In Review
assignee: []
created_date: '2026-11-10T14:39:00Z'
updated_date: '2026-11-10T15:07:00Z'
labels:
  - 94-impl
dependencies: []
parent_task_id: TASK-94
ordinal: 83000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 FocusContext (app id, app name, window title, own-app flag) is public in core behind the focus feature
- [x] #2 macOS returns bundle id and name without any permission, and the window title when Accessibility is granted
- [x] #3 A slow or hung app cannot stall the read beyond the messaging timeout
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
core/src/focus/{mod,mac}.rs behind the new 'focus' feature (pulled in by 'tauri', enabled by the CLI). NSWorkspace for app id and name, Accessibility (AXFocusedWindow, AXTitle) for the title with a 0.25 s messaging timeout.
Verified: cargo test (5 focus tests); openwhisper focus against a real terminal window returned bundle id, name and title; history entries written by the Dev app carried the app name.
<!-- SECTION:NOTES:END -->
