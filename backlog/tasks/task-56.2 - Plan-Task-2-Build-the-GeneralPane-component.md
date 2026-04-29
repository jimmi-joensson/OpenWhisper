---
id: TASK-56.2
title: 'Plan Task 2: Build the GeneralPane component'
status: Done
assignee:
  - '@claude'
created_date: '2026-04-29 08:26'
updated_date: '2026-04-29 13:31'
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
- [x] #1 general-pane.tsx exports GeneralPane built from shadcn Switch, ToggleGroup, Field family, and Separator
- [x] #2 Three sections render with the section-header treatment (mono, uppercase, muted-foreground)
- [x] #3 Updates row fetches core_version via invoke and renders in font-mono
- [x] #4 No space-x-*/space-y-* classes; no raw color overrides on shadcn components; no manual dark: modifiers
- [x] #5 pnpm tsc --noEmit clean
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
69f7845 TASK-56.2: GeneralPane scaffold — three sections, live core_version invoke
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
GeneralPane shipped with shadcn primitives. Plan-deviations: <h3> for section headers (FieldLegend default styling fights design's mono/uppercase/muted treatment); FieldContent wraps label+description so horizontal Field rows stack the hint under the label instead of placing it next to the control.
<!-- SECTION:FINAL_SUMMARY:END -->
