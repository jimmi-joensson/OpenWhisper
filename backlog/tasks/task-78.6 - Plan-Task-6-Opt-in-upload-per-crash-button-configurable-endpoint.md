---
id: TASK-78.6
title: 'Plan Task 6: Opt-in upload (per-crash button, configurable endpoint)'
status: To Do
assignee: []
created_date: '2026-05-04 06:16'
updated_date: '2026-05-04 08:03'
labels:
  - 78-impl
dependencies: []
parent_task_id: TASK-78
ordinal: 39000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 crashes_upload command exists and is the single upload path
- [ ] #2 Button disabled state with explanatory tooltip when endpoint unconfigured
- [ ] #3 First-upload confirm dialog lists the exact fields sent (matches spec wording)
- [ ] #4 state.json records uploaded_at on success; failed upload does not delete the file
<!-- AC:END -->
