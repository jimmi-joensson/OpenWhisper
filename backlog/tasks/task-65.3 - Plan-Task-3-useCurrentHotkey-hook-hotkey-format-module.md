---
id: TASK-65.3
title: 'Plan Task 3: useCurrentHotkey hook + hotkey-format module'
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
- [ ] #1 hotkey-format.ts exports configToChipKeys, modifierLabel, modShortLabel, codeLabel, formatHotkeyLabel.
- [ ] #2 useCurrentHotkey('toggle') returns live HotkeyConfig; updates on hotkey_captured events for the matching target.
- [ ] #3 Settings.tsx imports the lifted helpers; local duplicates removed; ShortcutsPane chip rendering unchanged.
- [ ] #4 settings-window.spec.ts stays green.
- [ ] #5 pnpm tsc --noEmit clean.
<!-- AC:END -->
