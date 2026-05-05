---
id: TASK-83.3
title: 'Plan Task 3: ISSUE_TEMPLATE/ — bug_report.yml, feature_request.yml, config.yml'
status: To Do
assignee: []
created_date: '2026-05-04 16:06'
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
Three YAML form templates at .github/ISSUE_TEMPLATE/. Bug-report fields encode platform-gotchas (App version, OS+version, CPU arch, recognizer engine, BT state). config.yml sets blank_issues_enabled: false and routes feature requests to GitHub Discussions (or maintainer email if Discussions not yet enabled).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 bug_report.yml committed; submitting without app version + OS + recognizer is blocked by GitHub form validation
- [ ] #2 feature_request.yml committed with Use case + Proposed approach + project-principles checkbox
- [ ] #3 config.yml sets blank_issues_enabled: false and includes Discussions link OR maintainer-email contact link (per pre-work decision)
- [ ] #4 Auto-labels (bug, triage, enhancement) defined in YAML so triage doesn't manually label every issue
- [ ] #5 GitHub New Issue flow shows only the two templates; Open a blank issue option is gone
- [ ] #6 bug_report.yml includes Audio output device text field (BT-Classic / LE Audio / wired matters for BT mono-tail gotcha)
- [ ] #7 bug_report.yml includes 'Was OpenWhisper's own window focused?' radio (Yes/No/N/A) — critical for hotkey/Esc-cancel triage on Windows
<!-- AC:END -->
