---
id: TASK-86.2
title: 'Plan Task 2: add shadcn Kbd primitive to apps/tauri'
status: To Do
assignee: []
created_date: '2026-05-06 05:12'
updated_date: '2026-05-06 05:17'
labels:
  - 86-impl
dependencies: []
parent_task_id: TASK-86
ordinal: 44000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 kbd.tsx committed under apps/tauri/src/components/ui/ via shadcn CLI
- [ ] #2 Project compiles cleanly with the new Kbd component imported and used in a real consumer (throwaway placement OK, removed in Task 3)
- [ ] #3 Visual smoke: <Kbd>⌘,</Kbd> renders with border + monospace font matching home-pane.tsx:83 styling
- [ ] #4 Existing Playwright suite still green; no spec depended on the absence of Kbd
<!-- AC:END -->
