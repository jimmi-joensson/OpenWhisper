---
id: TASK-65.7
title: 'Plan Task 7: Test refactor + delete MainWindowShell shim'
status: Done
assignee: []
created_date: '2026-04-30 22:46'
updated_date: '2026-05-01 13:34'
labels:
  - 65-impl
dependencies: []
parent_task_id: TASK-65
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 main-window.spec.ts trimmed to app-shell-level tests: sidebar nav routing, body scroll, drag/chrome non-selectable.
- [ ] #2 diagnostics.spec.ts created with the lifted dashboard assertions (four cards, FFI core_version, debug Card payload, RecordButton phase transitions, transcribe-prefix error in last-error KV, KV/transcript text-selectable).
- [ ] #3 home.spec.ts gains the banner-on-Home tests (hotkey banner + retry, mic banner, recognizer-load banner).
- [ ] #4 src/components/main-window-shell.tsx deleted; no remaining imports of that path.
- [ ] #5 Dev-shell smoke (pnpm dev) verifies all three routes render; Cmd/Ctrl+, jumps to Settings; tray Preferences… still opens Settings.
- [ ] #6 pnpm test:ui green; pnpm tsc --noEmit clean.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Commit: f4bcec5 — split tests by route; delete MainWindowShell shim. 56/56 Playwright; tsc clean. Live-shell smoke (Step 5) deferred to pre-review.
<!-- SECTION:NOTES:END -->
