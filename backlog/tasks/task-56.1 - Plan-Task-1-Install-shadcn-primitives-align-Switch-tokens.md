---
id: TASK-56.1
title: 'Plan Task 1: Install shadcn primitives + align Switch tokens'
status: Done
assignee:
  - '@claude'
created_date: '2026-04-29 08:25'
updated_date: '2026-04-29 13:29'
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
- [x] #1 switch.tsx, toggle-group.tsx, field.tsx, separator.tsx exist under apps/tauri/src/components/ui/
- [x] #2 Each new file passes the 'review added components' checklist (correct alias, no leaked RSC directive, lucide icons if any)
- [x] #3 App.css contains a CSS-variable-based override painting any checked Switch in var(--info)
- [x] #4 pnpm tsc --noEmit clean; existing Audio + Shortcuts panes render unchanged in dev
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
acf635a TASK-56.1: install shadcn primitives + info-blue Switch override
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Installed BaseUI-shaped switch/toggle-group/field/separator (plus toggle/label deps). App.css [data-slot=switch][data-checked] paints --info; Switch on-state will render macOS systemBlue. tsc clean. API notes for 56.2: Field orientation prop accepts vertical|horizontal|responsive (default vertical); FieldSet+FieldLegend exported; ToggleGroup uses BaseUI multiple boolean (default single) with array values.
<!-- SECTION:FINAL_SUMMARY:END -->
