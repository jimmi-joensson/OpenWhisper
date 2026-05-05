---
id: TASK-83
title: OSS community files
status: To Do
assignee: []
created_date: '2026-05-04 16:00'
updated_date: '2026-05-04 16:06'
labels: []
milestone: m-1
dependencies: []
documentation:
  - backlog/docs/specs/doc-28 - OSS-community-files-—-design.md
  - backlog/docs/plans/doc-29 - OSS-community-files-—-implementation-plan.md
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Bring .github/ + root scaffold up to bar contributors expect: SECURITY.md, CODE_OF_CONDUCT.md, ISSUE_TEMPLATE/, CODEOWNERS, PR template upgrade, dependabot.yml, CHANGELOG.md. Authoring tasks (static markdown), not engineering. Lands in parallel with TASK-81 / TASK-82. Stays under 'OpenWhisper' name; rename in TASK-NEW-5 final sweep.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 SECURITY.md committed with private GHSA reporting + 48h triage SLA + scoped attack surface (audio-file RCE / IPC / supply-chain / global keyboard hook abuse / mic stream interception)
- [ ] #2 CODE_OF_CONDUCT.md committed (Contributor Covenant 2.1) with maintainer contact email substituted in
- [ ] #3 .github/ISSUE_TEMPLATE/{bug_report.yml, feature_request.yml, config.yml} committed; bug fields encode platform-gotchas (App version, OS+version, CPU arch, recognizer engine, BT state when relevant, log excerpt). config.yml sets blank_issues_enabled: false and routes feature requests to GitHub Discussions
- [ ] #4 CODEOWNERS committed; claims core/ and apps/tauri/ ownership for the maintainer; auto-requests review on PRs
- [ ] #5 .github/PULL_REQUEST_TEMPLATE.md upgraded with cross-platform-test checkbox (Mac smoke / Windows smoke / pnpm test:ui), Backlog task ID field, AI-assistance disclosure — existing legal boilerplate preserved
- [ ] #6 .github/dependabot.yml committed: weekly github-actions, monthly cargo + npm
- [ ] #7 CHANGELOG.md committed in Keep-A-Changelog format; reverse-walks 0.3.0 → 0.5.0 + Unreleased section at top
<!-- AC:END -->
