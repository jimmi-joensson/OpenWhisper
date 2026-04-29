---
id: TASK-55.7
title: 'Plan Task 7: Playwright spec for the toggle'
status: Done
assignee:
  - '@claude'
created_date: '2026-04-29 08:01'
updated_date: '2026-04-29 19:05'
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
- [x] #1 Playwright assertion: toggle is ON by default
- [x] #2 Playwright assertion: flipping calls settings_set_pill_follow with { follow: false }
- [x] #3 Playwright assertion: toggle hydrates OFF when settings_get_pill returns follow_active_screen: false
- [ ] #4 Manual multi-monitor smoke steps documented in spec file or sibling MANUAL.md
- [ ] #5 pnpm test:ui runs green locally and on CI
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
d10b424 Playwright: 3 specs (default-ON, set-on-flip, hydrate-OFF) + shim handlers; 43/43 green.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added three Playwright tests in settings-window.spec.ts covering default-ON, set-on-flip (asserts settings_set_pill_follow invoked with { follow: false } via __owPillLastFollow / __owPillSetCount probes), and hydrate-OFF (addInitScript stashes __owPillFollow=false before goto). Tauri-shim got settings_get_pill / settings_set_pill_follow / reposition_pill (noop) handlers. Manual multi-monitor smoke steps documented as a // Manual: comment block above the new tests. pnpm test:ui passes 43/43.
<!-- SECTION:FINAL_SUMMARY:END -->
