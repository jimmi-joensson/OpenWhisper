---
id: TASK-83.1
title: 'Plan Task 1: SECURITY.md'
status: To Do
assignee: []
created_date: '2026-05-04 16:05'
updated_date: '2026-05-04 16:09'
labels:
  - 83-impl
milestone: m-1
dependencies: []
parent_task_id: TASK-83
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Author SECURITY.md at repo root, modeled on OpenWhispr structure. ~80 lines. Names GitHub Security Advisories as private reporting channel; states 48h triage SLA + 7-day fix target; enumerates 5 in-scope attack surfaces (audio-file RCE / IPC abuse / supply chain / keyboard hook / mic stream).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 SECURITY.md committed at repo root with Reporting a Vulnerability section that names GitHub Security Advisories
- [ ] #2 48h triage SLA + 7-day fix target stated explicitly
- [ ] #3 5 in-scope attack surfaces enumerated by name (audio-file RCE, IPC abuse, supply-chain, keyboard hook, mic stream)
- [ ] #4 Maintainer contact email is project-owned (not personal); reviewer blocks merge if placeholder or personal address
<!-- AC:END -->
