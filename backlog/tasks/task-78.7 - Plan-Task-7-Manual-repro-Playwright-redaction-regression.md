---
id: TASK-78.7
title: 'Plan Task 7: Manual repro + Playwright + redaction regression'
status: To Do
assignee: []
created_date: '2026-05-04 06:16'
updated_date: '2026-05-04 08:03'
labels:
  - 78-impl
dependencies: []
parent_task_id: TASK-78
ordinal: 40000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Playwright spec for crash inspector committed and green; covers list, detail, copy, mark-read, delete-all
- [ ] #2 Manual repro confirmed on macOS + Windows: panic → file → next-launch toast → list → copy → paste-clean markdown
- [ ] #3 Redaction regression test rejects PII-shaped strings in clipboard output
- [ ] #4 Project CLAUDE.md / apps CLAUDE.md updated to point at the new spec file
<!-- AC:END -->
