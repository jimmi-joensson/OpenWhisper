---
id: TASK-86.3
title: 'Plan Task 3: build StatusFooter and wire into App.tsx'
status: To Do
assignee: []
created_date: '2026-05-06 05:13'
updated_date: '2026-05-06 05:17'
labels:
  - 86-impl
dependencies:
  - TASK-86.1
  - TASK-86.2
parent_task_id: TASK-86
ordinal: 45000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Settings-hint click region routes to settings; keycap shows Ctrl+, on Windows and ⌘, on macOS
- [ ] #2 Status dot color and phrase reflect dictation.phase live (idle=green/Ready, recording=red/Recording, transcribing=amber/Transcribing, error=red/Error)
- [ ] #3 Engine name + origin appear after recognizer_info resolves; before that the middle region shows only dot + phrase
- [ ] #4 Right-side hotkey kbd updates within one render after a Settings rebind, no stale value flash
- [ ] #5 Footer outside ow-app__shell so it spans the full window width including the sidebar column
- [ ] #6 StatusFooter renders in all three routes (Home, Settings, Diagnostics); .ow-app__footer measures exactly 32 px tall in every route as verified by Playwright boundingBox().height
<!-- AC:END -->
