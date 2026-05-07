---
id: TASK-78.3
title: 'Plan Task 3: Diagnostics overview entry card + full-pane crash list'
status: In Review
assignee:
  - '@claude'
created_date: '2026-05-04 06:16'
updated_date: '2026-05-07 17:07'
labels:
  - 78-impl
milestone: m-1
dependencies: []
parent_task_id: TASK-78
ordinal: 41000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Diagnostics overview pane renders a Crashes entry card with live unread pill + last-crash summary, polled at 2 Hz
- [x] #2 Tapping the card swaps the Diagnostics pane to the crash list (no sub-sidebar, no nested rail), with a 'Diagnostics /' breadcrumb back to overview
- [x] #3 Crash list renders rows with hover-revealed [✓] mark-read + [🗑] delete; resting row shows chevron only; row click opens the detail sheet AND marks the crash read
- [x] #4 Single-row Delete is one-click (no confirm dialog); Delete-all uses shadcn AlertDialog with '<unread> will be removed' body
- [x] #5 Empty state replaces the entire pane with the empty composition and a single Open-crash-folder button; pane header is hidden in this state
- [x] #6 shadcn primitives used for AlertDialog + Tooltip + Button (per ui-discipline)
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
885a415 Diagnostics overview entry card + full-pane crash list (no sub-sidebar); 9 Playwright tests + 99/99 project suite green
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Crashes entry card + full-pane list landed in 885a415. Local view-state on DiagnosticsPane (overview | crashes) avoids router scope; entry card uses sunken card + destructive-tint glyph tile + recording-orange unread pill + relative sub-line + chevron, polled at 2 Hz via the new useCrashes hook. List pane: breadcrumb back + counts + Delete-all header, scrollable list with hover-revealed [✓]/[🗑] (no overflow menu), single-row delete one-click, Delete-all via shadcn AlertDialog with dynamic '<unread> will be removed too' copy. Row click opens right-side Sheet placeholder (78.4 fills the body) and fires mark-read on open. Empty state replaces the entire pane with the muted glyph tile + Open-crash-folder button; pane header hidden. shadcn AlertDialog + Tooltip + Button + Sheet used per ui-discipline. Tauri shim seeded with crashes_* command stubs; new tests/crash-inspector.spec.ts has 9 tests, all green; full project suite 99/99.
<!-- SECTION:FINAL_SUMMARY:END -->
