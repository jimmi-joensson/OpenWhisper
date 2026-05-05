---
id: TASK-85.8
title: 'Plan Task 8: Docs sweep (forward-looking only — backlog history stays)'
status: To Do
assignee: []
created_date: '2026-05-04 16:36'
labels:
  - 85-impl
dependencies: []
parent_task_id: TASK-85
milestone: m-1
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Replace OpenWhisper → <NEW_NAME> in every forward-looking doc: README, INSTALL.md, BUILD.md, NOTICE, CONTRIBUTING.md, CLAUDE.md, SECURITY.md, CODE_OF_CONDUCT.md, CHANGELOG.md, .github/* (preserving PR template legal boilerplate structure). Rename .claude/skills/openwhisper-* directories + frontmatter name: fields. Update CLAUDE.md prefix rule. Update apps/tauri/package.json name/homepage/repository/bugs. Backlog history left as-is — historical references.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 rg 'OpenWhisper' --glob '!backlog/' --glob '!archive/' --glob '!docs/release-*-handover.md' returns zero hits in forward-looking content
- [ ] #2 All .claude/skills/openwhisper-*/ directories renamed to <new>-*/; SKILL.md frontmatter name: fields updated
- [ ] #3 CLAUDE.md skill-prefix rule references <new>-* not openwhisper-*
- [ ] #4 PR template legal boilerplate structure preserved verbatim — only project name word substituted
- [ ] #5 apps/tauri/package.json name/homepage/repository.url/bugs.url updated to new GitHub URL (resolves after Task 9)
<!-- AC:END -->
