---
id: TASK-56.2
title: 'Plan Task 2: Build the GeneralPane component'
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
New apps/tauri/src/components/general-pane.tsx using only shadcn primitives. Three sections: Startup (placeholder Switch), Appearance (Theme stub ToggleGroup), Updates (live core_version readout). Local-state-only for placeholder rows.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 general-pane.tsx exports GeneralPane built from shadcn Switch, ToggleGroup, Field family, and Separator
- [ ] #2 Three sections render with the section-header treatment (mono, uppercase, muted-foreground)
- [ ] #3 Updates row fetches core_version via invoke and renders in font-mono
- [ ] #4 No space-x-*/space-y-* classes; no raw color overrides on shadcn components; no manual dark: modifiers
- [ ] #5 pnpm tsc --noEmit clean
<!-- AC:END -->
