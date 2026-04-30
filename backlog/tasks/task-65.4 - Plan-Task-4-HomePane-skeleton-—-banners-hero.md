---
id: TASK-65.4
title: 'Plan Task 4: HomePane skeleton — banners + hero'
status: To Do
assignee: []
created_date: '2026-04-30 22:45'
labels:
  - 65-impl
dependencies: []
parent_task_id: TASK-65
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 <HomePane> renders banners at the top above a centered hero (icon + 'Ready when you are' + chord-bearing hint).
- [ ] #2 Hero hotkey hint reads from useCurrentHotkey('toggle') + formatHotkeyLabel; default Mac binding renders 'Right ⌘'; updates live on hotkey_captured.
- [ ] #3 Banners (mic / hotkey / recognizer-load) removed from DiagnosticsPane; only HomePane renders them.
- [ ] #4 App icon copied to apps/tauri/src/assets/icon-128.png with a README noting src-tauri/icons/ as the source.
- [ ] #5 home.spec.ts (3 tests: hero + live hint update + banner-above-hero ordering) green.
- [ ] #6 pnpm tsc --noEmit clean.
<!-- AC:END -->
