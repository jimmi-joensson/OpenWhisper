---
id: TASK-56.4
title: 'Plan Task 4: Playwright spec — section structure + landing pane'
status: To Do
assignee: []
created_date: '2026-04-29 08:26'
labels:
  - 56-impl
dependencies: []
parent_task_id: TASK-56
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Extend settings-window.spec.ts to assert the three section headers, the three field labels, default-System Theme, and default-checked Launch at login Switch. Existing landing-on-General + sidebar tests stay green.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 New test asserts Startup, Appearance, About section headers and Launch at login / Theme / Current version labels render
- [ ] #2 Theme ToggleGroup default-selected value is 'system'
- [ ] #3 Launch at login Switch starts checked
- [ ] #4 Existing 'renders sidebar with all four panes' and 'General is the landing pane' tests still pass
- [ ] #5 pnpm test:ui green locally and on CI
<!-- AC:END -->
