---
id: TASK-78.4
title: 'Plan Task 4: Detail sheet + Copy-as-markdown + Open-folder'
status: In Progress
assignee:
  - '@claude'
created_date: '2026-05-04 06:16'
updated_date: '2026-05-07 17:15'
labels:
  - 78-impl
milestone: m-1
dependencies: []
parent_task_id: TASK-78
ordinal: 37000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Detail renders inside a right-side sheet (~580 px) over a dimmed backdrop; closing returns to the (now-read) list
- [ ] #2 Sheet has sticky header + sticky action footer; backtrace scroll never hides the primary Copy button
- [ ] #3 Opening the sheet marks the crash read; closing the sheet does not un-read; deleting from the sheet closes it and removes the row
- [ ] #4 Copy GitHub-ready report writes redacted markdown to the clipboard and inline-flips to '✓ Copied' for 1.2 s
- [ ] #5 Markdown formatter has Vitest coverage for: full-shape, missing recording_state, empty events, and a PII-shaped redaction-regression fixture
- [ ] #6 Open-crash-folder works on macOS + Windows via tauri-plugin-opener
<!-- AC:END -->
