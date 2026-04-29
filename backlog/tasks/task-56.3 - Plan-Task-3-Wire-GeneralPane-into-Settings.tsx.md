---
id: TASK-56.3
title: 'Plan Task 3: Wire GeneralPane into Settings.tsx'
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
Replace <PaneStub title='General' /> at Settings.tsx:99 with <GeneralPane />. Keep PaneStub helper for the Models pane. Verify visual treatment matches design via dev smoke.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Settings.tsx line 99 routes the General pane to the real GeneralPane component
- [ ] #2 PaneStub helper retained and still used by the Models pane
- [ ] #3 AudioPane and ShortcutsPane render unchanged
- [ ] #4 Manual smoke: info-blue Switch when checked, mono section headers, three sections visible
<!-- AC:END -->
