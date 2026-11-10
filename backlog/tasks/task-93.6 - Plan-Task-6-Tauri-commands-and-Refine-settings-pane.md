---
id: TASK-93.6
title: 'Plan Task 6: Tauri commands and Refine settings pane'
status: In Review
assignee: []
created_date: '2026-11-10T09:14:00Z'
updated_date: '2026-11-10T09:48:00Z'
labels:
  - 93-impl
dependencies: []
parent_task_id: TASK-93
ordinal: 70000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Pane has on/off switch (off by default), backend picker, instructions with reset, and test connection with latency or failure reason
- [x] #2 Pane states plainly where text is sent for the selected backend
- [x] #3 CLI not installed and not signed in each show a clear message
- [x] #4 API key field writes to the keychain and never echoes the stored key
- [x] #5 Home shows refined or fell-back state and gives access to the unrefined text
- [x] #6 Playwright spec covers the pane and the Home marker
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
refine_* Tauri commands, Refine pane, Home marker.
Verification: pnpm tsc --noEmit clean; pnpm exec playwright test -> 150 passed (14 new in refine.spec.ts); vitest 22 passed; cargo check -p openwhisper-tauri clean (macOS and x86_64-pc-windows-gnu).
The pane is verified against the test shim only. The real commands in the running app await user QA. The design canvas (tab 12 - Models) could not be read from this session (owned by another account, needs an interactive approval), so the layout follows the existing settings panes.
<!-- SECTION:NOTES:END -->
