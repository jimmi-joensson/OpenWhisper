---
id: TASK-82.1
title: 'Plan Task 1: ci.yml skeleton + triggers + concurrency'
status: To Do
assignee: []
created_date: '2026-05-04 15:46'
labels:
  - 82-impl
dependencies: []
parent_task_id: TASK-82
milestone: m-1
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Land the workflow file with three stub jobs (rust-gate-mac, rust-gate-win, frontend-gate-mac), trigger config (PR + push to main + workflow_dispatch), and concurrency cancellation. No real gates yet; subsequent tasks fill them in.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 .github/workflows/ci.yml committed with three jobs and trigger boilerplate
- [ ] #2 PR opening triggers three GitHub status checks named rust-gate-mac, rust-gate-win, frontend-gate-mac — all pass on stub
- [ ] #3 concurrency group cancels stale runs when new commits land on the same PR branch
- [ ] #4 Workflow appears in the Actions tab as 'CI'
<!-- AC:END -->
