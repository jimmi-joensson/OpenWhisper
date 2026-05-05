---
id: TASK-83.4
title: 'Plan Task 4: CODEOWNERS'
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
Tiny file at .github/CODEOWNERS. Single-maintainer ownership for v1: default *, plus explicit signals for core/, apps/tauri/, .github/, backlog/.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 .github/CODEOWNERS committed; maintainer GH handle owns * and the four explicit path globs
- [ ] #2 PR from any author auto-requests review from the maintainer
- [ ] #3 GitHub Settings → Code security → CODEOWNERS shows no parse errors
<!-- AC:END -->
