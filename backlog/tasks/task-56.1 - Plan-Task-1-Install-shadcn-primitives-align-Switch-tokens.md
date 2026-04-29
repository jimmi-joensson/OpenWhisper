---
id: TASK-56.1
title: 'Plan Task 1: Install shadcn primitives + align Switch tokens'
status: To Do
assignee: []
created_date: '2026-04-29 08:25'
labels:
  - 56-impl
dependencies: []
parent_task_id: TASK-56
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Install switch, toggle-group, field, separator from shadcn @shadcn registry; review each per the shadcn skill workflow; add an info-blue Switch checked-state override in App.css via CSS variable (not per-instance className).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 switch.tsx, toggle-group.tsx, field.tsx, separator.tsx exist under apps/tauri/src/components/ui/
- [ ] #2 Each new file passes the 'review added components' checklist (correct alias, no leaked RSC directive, lucide icons if any)
- [ ] #3 App.css contains a CSS-variable-based override painting any checked Switch in var(--info)
- [ ] #4 pnpm tsc --noEmit clean; existing Audio + Shortcuts panes render unchanged in dev
<!-- AC:END -->
