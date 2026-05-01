---
id: TASK-56
title: Settings — General pane scaffold
status: Done
assignee: []
created_date: '2026-04-29 08:24'
updated_date: '2026-05-01 08:13'
labels: []
dependencies: []
documentation:
  - backlog/docs/specs/2026-04-29-general-pane-scaffold.md
  - backlog/docs/plans/2026-04-29-general-pane-scaffold.md
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Replace the PaneStub at Settings.tsx:99 with a real GeneralPane built from shadcn primitives, matching the design's SettingsGeneralBoard. Establishes the Switch / ToggleGroup / Field / Separator vocabulary so subsequent feature tasks (TASK-54 launch-at-login, TASK-55.6 follow-active-screen, future Sound FX / Updates / Show-in-Dock) drop into a real pane instead of into a stub. Rows for features without a backing task are intentionally omitted.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 PaneStub for General is replaced with a real GeneralPane component
- [x] #2 Pane uses shadcn Switch, ToggleGroup, Field family, and Separator — no custom toggle markup
- [x] #3 Three sections render: Startup (Launch at login placeholder Switch), Appearance (Theme stub ToggleGroup), Updates (live current version)
- [x] #4 Switch's checked state paints in the design's info-blue, not shadcn's default primary
- [ ] #5 Lucide icons used for sidebar items if iconography lands in scope (else deferred)
- [x] #6 Existing Settings landing-on-General + sidebar tests still pass; new tests cover the section structure
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
General pane scaffold landed — 4 subtasks done, 39/39 Playwright tests pass. AC#5 (lucide sidebar icons) deferred: sidebar already shipped with emoji glyphs (project_recognizer_tauri PANES const), no iconography refactor in scope. Subsequent feature tasks (TASK-54 launch-at-login, TASK-55.6 follow-active-screen) now land into a real pane.
<!-- SECTION:FINAL_SUMMARY:END -->
