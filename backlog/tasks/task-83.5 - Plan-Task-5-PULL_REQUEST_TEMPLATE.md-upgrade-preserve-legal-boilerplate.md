---
id: TASK-83.5
title: 'Plan Task 5: PULL_REQUEST_TEMPLATE.md upgrade (preserve legal boilerplate)'
status: To Do
assignee: []
created_date: '2026-05-04 16:06'
labels:
  - 83-impl
dependencies: []
parent_task_id: TASK-83
milestone: m-1
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Edit existing .github/PULL_REQUEST_TEMPLATE.md. Add: Backlog task ID, cross-platform-smoke checkboxes (Mac smoke / Win smoke / Playwright / cargo test), AI-disclosure section, CHANGELOG-updated checkbox. Preserve legal boilerplate verbatim — it's load-bearing for contribution rights.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Backlog task ID field added (TASK-N or 'no task')
- [ ] #2 Cross-platform smoke checklist with Mac smoke / Win smoke / pnpm test:ui / cargo test
- [ ] #3 AI-assistance disclosure section added (optional, Handy-style)
- [ ] #4 Updated CHANGELOG.md (or N/A) checkbox added
- [ ] #5 Legal boilerplate preserved verbatim — byte-level diff against the previous version shows no edits to the legal section (except project name)
<!-- AC:END -->
