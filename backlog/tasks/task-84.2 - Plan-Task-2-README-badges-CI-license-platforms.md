---
id: TASK-84.2
title: 'Plan Task 2: README badges (CI / license / platforms)'
status: To Do
assignee: []
created_date: '2026-05-04 16:22'
labels:
  - 84-impl
dependencies: []
parent_task_id: TASK-84
milestone: m-1
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add 4-badge row to README directly below title: CI status (TASK-82 workflow URL), MIT license (shields.io static), macOS 15+ pill, Windows 10/11 pill. CI badge renders 'no status' before TASK-82 ships — acceptable, not broken.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 4-badge row committed at top of README directly below title
- [ ] #2 CI badge URL points at TASK-82's ci.yml workflow path
- [ ] #3 License + platform badges render correctly on GitHub via shields.io static URLs
- [ ] #4 No broken-image badges on render
<!-- AC:END -->
