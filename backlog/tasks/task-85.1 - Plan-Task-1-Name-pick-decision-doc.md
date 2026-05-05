---
id: TASK-85.1
title: 'Plan Task 1: Name pick + decision doc'
status: To Do
assignee: []
created_date: '2026-05-04 16:35'
labels:
  - 85-impl
dependencies: []
parent_task_id: TASK-85
milestone: m-1
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Maintainer picks new name from research candidates (Murmur / Parley / Tellur or own pick). Verifies external availability across GitHub/domain/npm/brew/winget/socials (registration in Task 2). Records choice + rationale + placeholder mapping table as backlog/decisions/decision-N - Rename to <NEW_NAME>.md.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 backlog/decisions/decision-N - Rename to <NEW_NAME>.md committed via backlog decision create
- [ ] #2 ADR explains why old name is wrong (OpenWhispr conflict) + why this name + namespace availability per channel
- [ ] #3 Placeholder mapping table embedded in ADR (NEW_NAME / new_name / new / new-cargo / new-org-or-user)
- [ ] #4 No code changes in this commit — pure ADR
<!-- AC:END -->
