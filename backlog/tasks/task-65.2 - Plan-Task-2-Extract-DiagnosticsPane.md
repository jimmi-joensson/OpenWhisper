---
id: TASK-65.2
title: 'Plan Task 2: Extract DiagnosticsPane'
status: In Review
assignee: []
created_date: '2026-04-30 22:45'
updated_date: '2026-05-01 13:52'
labels:
  - 65-impl
dependencies: []
parent_task_id: TASK-65
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 New <DiagnosticsPane> mirrors the previous debug dashboard (FFI / Dictation debug / mic→Parakeet / transcript / RecordButton); centered title <h1> dropped.
- [ ] #2 Diagnostics route renders DiagnosticsPane; Home route renders a placeholder ('Home pane — coming in Task 4').
- [ ] #3 main-window-shell.tsx becomes a thin re-export shim of DiagnosticsPane (deleted in Task 7).
- [ ] #4 Existing main-window.spec.ts assertions navigate via Diagnostics sidebar click; suite green.
- [ ] #5 pnpm tsc --noEmit clean.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Commit: 0d26154 — extract DiagnosticsPane; Home placeholder. 48/48 Playwright; tsc clean.
<!-- SECTION:NOTES:END -->
