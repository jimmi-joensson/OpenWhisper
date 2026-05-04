---
id: TASK-83.6
title: 'Plan Task 6: dependabot.yml (after TASK-82 lands)'
status: To Do
assignee: []
created_date: '2026-05-04 16:06'
updated_date: '2026-05-04 16:09'
labels:
  - 83-impl
milestone: m-1
dependencies:
  - TASK-82
parent_task_id: TASK-83
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
.github/dependabot.yml: weekly github-actions, monthly cargo + npm. Depends on TASK-82 landing first so Dependabot PRs hit a working CI gate. Labels each PR with 'dependencies' + ecosystem-specific tag for filtering.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 .github/dependabot.yml committed with three update entries (github-actions weekly, cargo monthly, npm monthly)
- [ ] #2 Each Dependabot PR labeled 'dependencies' + ecosystem-specific (github-actions / rust / frontend)
- [ ] #3 First Dependabot PR opens against main within a week of merge; CI workflow runs on it
- [ ] #4 Lands AFTER TASK-82 is green on main
<!-- AC:END -->
