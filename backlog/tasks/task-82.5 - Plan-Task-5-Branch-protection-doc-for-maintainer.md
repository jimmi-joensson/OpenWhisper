---
id: TASK-82.5
title: 'Plan Task 5: Branch-protection doc for maintainer'
status: To Do
assignee: []
created_date: '2026-05-04 15:47'
labels:
  - 82-impl
dependencies: []
parent_task_id: TASK-82
milestone: m-1
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Author docs/maintainer/branch-protection.md so the maintainer can apply GitHub UI settings post-rename / public-flip without referencing the implementation plan. Covers required PR review, status checks, conversation resolution, force-push lockdown, plus 3-item troubleshooting section.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 docs/maintainer/branch-protection.md committed
- [ ] #2 Doc names the three required status checks exactly as Tasks 2 and 3 emit them
- [ ] #3 Maintainer can follow the doc top-to-bottom without referencing the plan or spec
- [ ] #4 Troubleshooting section addresses: pnpm-lock.yaml drift, partial ort cache restore, cargo registry size limit
<!-- AC:END -->
