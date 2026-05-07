---
id: TASK-78.7
title: 'Plan Task 7: Manual repro + Playwright + redaction regression'
status: To Do
assignee: []
created_date: '2026-05-04 06:16'
updated_date: '2026-05-07 14:02'
labels:
  - 78-impl
milestone: m-1
dependencies: []
parent_task_id: TASK-78
ordinal: 40000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Manual repro confirmed on macOS + Windows: panic → file → next-launch toast → list → copy → paste-clean markdown
- [ ] #2 Redaction regression test rejects PII-shaped strings in clipboard output
- [ ] #3 Project CLAUDE.md / apps CLAUDE.md updated to point at the new spec file
- [ ] #4 Playwright spec for crash inspector committed and green; covers Diagnostics overview Crashes entry card, full-pane list, right-side detail sheet open/close, mark-read on sheet open, delta-driven launch toast (fires once, suppressed on no-delta restart), per-endpoint upload-dialog suppress checkbox persistence, single-row delete (no confirm), Delete-all confirm dialog, empty-state composition
<!-- AC:END -->
