---
id: TASK-78.4
title: 'Plan Task 4: Detail sheet + Copy-as-markdown + Open-folder'
status: Done
assignee:
  - '@claude'
created_date: '2026-05-04 06:16'
updated_date: '2026-05-07 22:22'
labels:
  - 78-impl
milestone: m-1
dependencies: []
parent_task_id: TASK-78
ordinal: 37000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Detail renders inside a right-side sheet (~580 px) over a dimmed backdrop; closing returns to the (now-read) list
- [x] #2 Sheet has sticky header + sticky action footer; backtrace scroll never hides the primary Copy button
- [x] #3 Opening the sheet marks the crash read; closing the sheet does not un-read; deleting from the sheet closes it and removes the row
- [x] #4 Copy GitHub-ready report writes redacted markdown to the clipboard and inline-flips to '✓ Copied' for 1.2 s
- [x] #5 Markdown formatter has Vitest coverage for: full-shape, missing recording_state, empty events, and a PII-shaped redaction-regression fixture
- [x] #6 Open-crash-folder works on macOS + Windows via tauri-plugin-opener
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
680b5c0 detail sheet body + Copy-as-markdown + Open-folder + design fidelity pass; 7 Vitest + 6 new Playwright tests
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Detail sheet body + Copy-as-markdown + Open-folder landed in 680b5c0, paired with a design-fidelity pass on the 78.3 surfaces against the new Crashes.html handoff. Sheet has sticky header with bordered close, scrollable body (identity / backtrace / collapsible Events), sticky footer surviving backtrace scroll (Copy GitHub-ready report flips to ✓ Copied for 1.2s, Open crash folder, Delete destructive-ghost). Pure crash-markdown formatter with Vitest covering full shape, missing recording_state, empty events, PII-shaped redaction regression, and markdown-safety (pipe escape, newline collapse). New crashes_open_folder Tauri command goes through tauri-plugin-opener. Diagnostics overview entry card + row + empty state rebuilt against design tokens (star glyph in destructive-tint tile, two-line breadcrumb block, 24×24 bordered hover actions, surface-sunken backdrops). 105/105 Playwright + 7/7 Vitest + 13/13 Rust crashes tests green; release-build TS + cargo check clean.
<!-- SECTION:FINAL_SUMMARY:END -->
