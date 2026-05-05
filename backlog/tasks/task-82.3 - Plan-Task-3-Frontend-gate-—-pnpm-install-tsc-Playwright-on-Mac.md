---
id: TASK-82.3
title: 'Plan Task 3: Frontend gate — pnpm install, tsc, Playwright on Mac'
status: To Do
assignee: []
created_date: '2026-05-04 15:47'
updated_date: '2026-05-04 15:51'
labels:
  - 82-impl
milestone: m-1
dependencies: []
parent_task_id: TASK-82
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Fill in frontend-gate-mac with the frontend gates: pnpm install --frozen-lockfile, pnpm exec tsc --noEmit, pnpm test:ui (Playwright). Mac-only — Tauri WebView2 on windows-latest is too brittle for v1.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 frontend-gate-mac runs tsc --noEmit and Playwright suite from apps/tauri/ on every PR
- [ ] #2 tsc error in any .ts/.tsx under apps/tauri/src/ turns the job red
- [ ] #3 Failing Playwright spec under apps/tauri/tests/*.spec.ts turns the job red
- [ ] #4 pnpm-lock.yaml drift (committing package.json without regenerated lockfile) turns the job red via --frozen-lockfile
- [ ] #5 pnpm/action-setup pinned to v4 with version 10
- [ ] #6 Job-level defaults set working-directory to apps/tauri so all pnpm steps find the only package.json
<!-- AC:END -->
