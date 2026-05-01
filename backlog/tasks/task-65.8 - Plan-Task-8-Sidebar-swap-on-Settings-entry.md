---
id: TASK-65.8
title: 'Plan Task 8: Sidebar swap on Settings entry'
status: Done
assignee: []
created_date: '2026-05-01 13:57'
updated_date: '2026-05-01 14:22'
labels: []
dependencies: []
parent_task_id: TASK-65
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
When route=settings, the outer sidebar replaces its Home/Settings/Diagnostics items with the four Settings pane items (General/Audio/Models/Shortcuts). Back arrow restores the outer sidebar. SettingsShell drops its inner sub-sidebar; pane state lifts up. Keeps role=tab + aria-selected + arrow-key cycle so the existing settings sidebar tests keep working.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 On route=settings, the outer sidebar renders General/Audio/Models/Shortcuts; clicking switches the visible pane.
- [ ] #2 On route=home or diagnostics, the outer sidebar renders Home/Settings/Diagnostics.
- [ ] #3 Back arrow on the settings titlebar returns to home and the outer sidebar restores.
- [ ] #4 SettingsShell renders pane content only — no inner sub-sidebar div.
- [ ] #5 ArrowDown/ArrowUp cycles through settings panes when focus is in the sidebar (parity with the existing inner-sidebar behavior).
- [ ] #6 Playwright suite green: existing settings sub-sidebar role=tab assertions still pass; new spec covers the route-conditional sidebar rendering.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Sidebar swaps on Settings entry; back arrow restores route sidebar. SettingsShell drops its inner sub-sidebar; pane state lifts to App.tsx. role=tab + aria-selected + ArrowDown/Up cycle preserved on the route-aware SidebarNav. 57/57 Playwright; tsc clean. Awaiting user QA in pnpm dev:tauri.
<!-- SECTION:NOTES:END -->
