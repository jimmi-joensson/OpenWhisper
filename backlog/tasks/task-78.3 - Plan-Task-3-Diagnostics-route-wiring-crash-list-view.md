---
id: TASK-78.3
title: 'Plan Task 3: Diagnostics overview entry card + full-pane crash list'
status: In Progress
assignee:
  - '@claude'
created_date: '2026-05-04 06:16'
updated_date: '2026-05-07 16:59'
labels:
  - 78-impl
milestone: m-1
dependencies: []
parent_task_id: TASK-78
ordinal: 41000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Diagnostics overview pane renders a Crashes entry card with live unread pill + last-crash summary, polled at 2 Hz
- [ ] #2 Tapping the card swaps the Diagnostics pane to the crash list (no sub-sidebar, no nested rail), with a 'Diagnostics /' breadcrumb back to overview
- [ ] #3 Crash list renders rows with hover-revealed [✓] mark-read + [🗑] delete; resting row shows chevron only; row click opens the detail sheet AND marks the crash read
- [ ] #4 Single-row Delete is one-click (no confirm dialog); Delete-all uses shadcn AlertDialog with '<unread> will be removed' body
- [ ] #5 Empty state replaces the entire pane with the empty composition and a single Open-crash-folder button; pane header is hidden in this state
- [ ] #6 shadcn primitives used for AlertDialog + Tooltip + Button (per ui-discipline)
<!-- AC:END -->
