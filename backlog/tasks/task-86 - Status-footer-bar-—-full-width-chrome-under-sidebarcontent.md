---
id: TASK-86
title: Status footer bar — full-width chrome under sidebar+content
status: To Do
assignee: []
created_date: '2026-05-06 05:09'
updated_date: '2026-05-06 05:13'
labels: []
dependencies: []
documentation:
  - backlog/docs/specs/doc-37 - Status-footer-bar-—-design.md
  - backlog/docs/plans/doc-38 - Status-footer-bar-—-implementation-plan.md
ordinal: 42000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add a full-width status footer that sits under both the sidebar and the content column. Mirrors the screenshot mock: left-aligned ⌘, SETTINGS hint inside the sidebar column, center-left status group (green dot + Ready · Parakeet · on-device), right-aligned Hotkey label + keycap. Pure UI consuming existing Rust state for phase + hotkey; engine name + on-device origin are new bits to surface from the recognizer.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Footer renders full window width, fixed height ~32 px, sits below ow-app__shell so it spans sidebar+content
- [ ] #2 Left section (in sidebar column): ⌘, SETTINGS keyboard hint using shadcn Kbd; click jumps to Settings
- [ ] #3 Center section: status dot color reflects dictation phase (idle=green, recording=red, transcribing=amber, error=red); status text matches phase; engine name and on-device origin shown after middot separators
- [ ] #4 Right section: 'Hotkey' label + current toggle hotkey rendered with shadcn Kbd, derived from useCurrentHotkey('toggle')
- [ ] #5 Engine name + on-device origin exposed from Rust via a new recognizer_info Tauri cmd; React hook subscribes once at mount
- [ ] #6 Playwright spec covers: empty-state footer renders all three groups, ⌘, click navigates to Settings, hotkey kbd updates after rebind
<!-- AC:END -->
