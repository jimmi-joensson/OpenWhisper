---
id: TASK-55.7
title: 'Plan Task 7: Playwright spec for the toggle'
status: To Do
assignee: []
created_date: '2026-04-29 08:01'
labels:
  - 55-impl
dependencies: []
parent_task_id: TASK-55
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Cover the UI half with Playwright. Multi-monitor follow behavior itself is not CI-testable; document manual smoke steps in the spec file.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Playwright assertion: toggle is ON by default
- [ ] #2 Playwright assertion: flipping calls settings_set_pill_follow with { follow: false }
- [ ] #3 Playwright assertion: toggle hydrates OFF when settings_get_pill returns follow_active_screen: false
- [ ] #4 Manual multi-monitor smoke steps documented in spec file or sibling MANUAL.md
- [ ] #5 pnpm test:ui runs green locally and on CI
<!-- AC:END -->
