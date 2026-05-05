---
id: TASK-85
title: Rename sweep — disambiguate from OpenWhispr
status: To Do
assignee: []
created_date: '2026-05-04 16:30'
updated_date: '2026-05-04 16:36'
labels: []
milestone: m-1
dependencies:
  - TASK-81
  - TASK-82
  - TASK-83
  - TASK-84
documentation:
  - backlog/docs/specs/doc-32 - Rename-sweep-—-design.md
  - backlog/docs/plans/doc-33 - Rename-sweep-—-implementation-plan.md
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Last task in m-1. Rename 'OpenWhisper' → <new-name> across the repo, the bundle, user-installed paths, and external presence. Parameterized on <new-name> / <new-name-lowercase> / <new-bundle-suffix>; name picked at execution time per maintainer decision. Single coordinated PR with one commit per subtask so repo never sits half-renamed. Mac TCC grants survive bundle-id change because Team ID 898R9M89GU stays stable. Settings dir migration shim preserves existing v0.5.x users' data.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Decision doc backlog/decisions/decision-N - Rename to <new-name>.md committed; new name picked + namespace availability verified
- [ ] #2 External namespace reservation checklist executed by maintainer (GitHub org/user, domain, brew cask, winget, npm, social handles)
- [ ] #3 Cargo workspace renamed: openwhisper-core → <new>-core, openwhisper-tauri → <new>-tauri, openwhisper-cli → <new>-cli. cargo check --workspace clean.
- [ ] #4 Mac bundle id renamed (com.openwhisper.app → com.<new>.app); CFBundleName/CFBundleExecutable updated; Team ID 898R9M89GU preserved; signed build still passes notarization
- [ ] #5 Windows bundle renamed (MSI product name, install dir, %APPDATA% path, registry); MSI builds clean
- [ ] #6 All user-visible 'OpenWhisper' strings replaced (tray, Pill, Settings UI, window titles, error toasts, log subsystem)
- [ ] #7 Settings/data migration shim ships: detects old ~/Library/Application Support/com.openwhisper.app/, copies to new path, writes idempotent marker. Mirror for Win %APPDATA%. Old dir scheduled for grace-period deletion.
- [ ] #8 All forward-looking docs updated (README, INSTALL.md, BUILD.md, NOTICE, CONTRIBUTING.md, CLAUDE.md, every .claude/skills/openwhisper-* renamed to <new>-*, package.json fields). Backlog history left as-is.
- [ ] #9 GitHub repo renamed; old URL redirects work or are documented; README badge URLs + dependabot.yml dirs updated
<!-- AC:END -->
