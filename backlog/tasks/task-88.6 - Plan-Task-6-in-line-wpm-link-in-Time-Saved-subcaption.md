---
id: TASK-88.6
title: 'Plan Task 6: in-line wpm link in Time Saved subcaption'
status: To Do
assignee: []
created_date: '2026-05-06 06:15'
labels:
  - 88-impl
dependencies:
  - TASK-88.4
  - TASK-88.5
parent_task_id: TASK-88
ordinal: 58000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 When stats nonzero, Time Saved subcaption renders 'vs. typing at <wpm> wpm' with the wpm portion as shadcn Button variant=link + lucide Settings icon at data-icon=inline-end + h-auto p-0 text-xs font-normal className override
- [ ] #2 Click navigates to route='settings' AND settingsPane='stats' (same pattern as ⌘, shortcut)
- [ ] #3 Empty state subcaption renders plain 'vs. typing' — no link, no number, no icon
- [ ] #4 Link styling uses the shadcn link variant tokens (text-primary underline-offset-4 hover:underline); no custom color overrides
<!-- AC:END -->
