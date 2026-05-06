---
id: TASK-86.4
title: 'Plan Task 4: Playwright spec for footer (empty / settings-click / rebind)'
status: To Do
assignee: []
created_date: '2026-05-06 05:13'
updated_date: '2026-05-06 05:17'
labels:
  - 86-impl
dependencies:
  - TASK-86.3
parent_task_id: TASK-86
ordinal: 46000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 apps/tauri/tests/status-footer.spec.ts added with three cases: empty-state render, Settings-hint click navigates, hotkey rebind reflects in footer kbd
- [ ] #2 Test 3 fails if StatusFooter is not re-reading from useCurrentHotkey after rebind (regression guard)
- [ ] #3 New spec is discovered by Playwright config and the project's pnpm test:ui run reports it as passing — actually executed, not inferred from the file (per CLAUDE.md)
- [ ] #4 Layout-stability assertion in case 1 reads .ow-app__footer boundingBox().height === 32 on Home and Settings routes, locking the Task 3 layout AC
<!-- AC:END -->
