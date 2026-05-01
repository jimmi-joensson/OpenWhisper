---
id: TASK-65.1
title: 'Plan Task 1: Outer sidebar nav + view-enum widening'
status: Done
assignee: []
created_date: '2026-04-30 22:45'
updated_date: '2026-05-01 14:22'
labels:
  - 65-impl
dependencies: []
parent_task_id: TASK-65
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 New <SidebarNav> renders Home / Settings / Diagnostics with lucide icons and aria-current on the active item.
- [ ] #2 View enum widens to 'home' | 'settings' | 'diagnostics'; default route is 'home'; ow_navigate maps 'main' → 'home'.
- [ ] #3 Titlebar gear button removed; settings reachable via sidebar (and existing tray Preferences… → ow_navigate 'settings' path still works).
- [ ] #4 New 'sidebar nav' Playwright test passes; existing main-window + settings-window specs stay green.
- [ ] #5 pnpm tsc --noEmit clean.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Commit: abed3d9 — Tauri: outer sidebar nav + Route enum (Home/Settings/Diagnostics). All steps complete; Playwright suite 48/48 green; tsc clean.
<!-- SECTION:NOTES:END -->
