---
id: TASK-56.3
title: 'Plan Task 3: Wire GeneralPane into Settings.tsx'
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
Replace <PaneStub title='General' /> at Settings.tsx:99 with <GeneralPane />. Keep PaneStub helper for the Models pane. Verify visual treatment matches design via dev smoke.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Settings.tsx line 99 routes the General pane to the real GeneralPane component
- [x] #2 PaneStub helper retained and still used by the Models pane
- [x] #3 AudioPane and ShortcutsPane render unchanged
- [x] #4 Manual smoke: info-blue Switch when checked, mono section headers, three sections visible
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
ae0debf TASK-56.3: route Settings General to GeneralPane

2c45368 TASK-56.4 verifies 56.3 visually: 39/39 Playwright pass
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Settings.tsx routes General pane to GeneralPane; PaneStub retained for Models. tsc clean. Visual smoke covered by TASK-56.4's full Playwright suite — all 39 tests pass, including the new section-structure assertions.
<!-- SECTION:FINAL_SUMMARY:END -->
