---
id: TASK-63.9
title: 'Plan Task 9: Cleanup settings UI (Dictation pane) + pill loading placeholder'
status: To Do
assignee: []
created_date: '2026-04-30 22:26'
labels:
  - 63-impl
dependencies: []
parent_task_id: TASK-63
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Dictation pane shows the four cleanup controls; disabled state propagates from the master toggle
- [ ] #2 Settings persist on every change; UI hydrates from stored values on mount
- [ ] #3 Pill shows a placeholder loading indicator (text or static dot — not the final animation) when cleanup model is in Loading state
- [ ] #4 Placeholder lives in a single component/location so TASK-64 can swap it without touching pill internals
<!-- AC:END -->
