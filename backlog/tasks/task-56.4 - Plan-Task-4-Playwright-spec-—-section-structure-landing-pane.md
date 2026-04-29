---
id: TASK-56.4
title: 'Plan Task 4: Playwright spec — section structure + landing pane'
status: Done
assignee:
  - '@claude'
created_date: '2026-04-29 08:26'
updated_date: '2026-04-29 13:34'
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
- [x] #1 New test asserts Startup, Appearance, Updates section headers and Launch at login / Theme / Current version labels render
- [x] #2 Theme ToggleGroup default-selected value is 'system'
- [x] #3 Launch at login Switch starts checked
- [x] #4 Existing 'renders sidebar with all four panes' and 'General is the landing pane' tests still pass
- [x] #5 pnpm test:ui green locally and on CI
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2c45368 TASK-56.4: section + defaults coverage; 39/39 pass
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Three new tests + landing-test update. Section structure assertion uses heading role (Startup/Appearance/Updates h3s). Theme defaults to System via aria-pressed; Launch at login starts checked via role=switch. 39/39 Playwright pass.
<!-- SECTION:FINAL_SUMMARY:END -->
