---
id: TASK-85.9
title: 'Plan Task 9: GitHub repo rename + URL fixups'
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
Maintainer renames GitHub repo (Settings → Repository name). Within-user rename auto-redirects old URLs forever; cross-user/org rename requires manual archive-readme at old slug. Verify redirect via curl. Substitute <org>/<repo> placeholders in README badge URLs, ISSUE_TEMPLATE config.yml, CHANGELOG footer tag-comparison links, dependabot.yml directories. Critical-section namespaces from Task 2 must be claimed before this runs.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 GitHub repo renamed; old jimmi-joensson/OpenWhisper URL returns 301 OR archive-readme exists at old slug
- [ ] #2 All README badge URLs work (CI badge resolves to real workflow page)
- [ ] #3 package.json homepage/repository/bugs URLs return 200 OK
- [ ] #4 Issue template config.yml Discussions URL works
- [ ] #5 CHANGELOG.md tag-comparison links resolve
- [ ] #6 External links (Hacker News submissions, blog posts, social) updated where the maintainer controls them
<!-- AC:END -->
