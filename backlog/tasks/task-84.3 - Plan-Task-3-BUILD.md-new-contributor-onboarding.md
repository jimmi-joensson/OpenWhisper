---
id: TASK-84.3
title: 'Plan Task 3: BUILD.md (new, contributor onboarding)'
status: To Do
assignee: []
created_date: '2026-05-04 16:22'
updated_date: '2026-05-04 16:26'
labels:
  - 84-impl
milestone: m-1
dependencies: []
parent_task_id: TASK-84
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Author BUILD.md at repo root (~250-300 lines). Single canonical contributor doc: prereqs (fnm, Rust, pnpm v10, Xcode CLT / MSVC), clone, first-time setup with pnpm setup:ort, dev loop on Mac (scripts/dev-run.sh — TCC drift rationale), dev loop on Windows, production build, tests with TASK-82's flag set, cross-platform gotchas link, Backlog.md task model, footer linking dev-workflow skill as source of truth.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 BUILD.md committed at repo root with prereqs → first PR walkthrough
- [ ] #2 Doc explains why scripts/dev-run.sh exists (TCC drift on pnpm tauri dev) with one-liner pointer to skill
- [ ] #3 Tests section uses exact cargo flags from TASK-82.2: --workspace --exclude bench-sherpa --no-default-features --features tauri
- [ ] #4 Cross-platform gotchas section links to .claude/skills/openwhisper-platform-gotchas/SKILL.md
- [ ] #5 Footer cites .claude/skills/openwhisper-dev-workflow/SKILL.md as source of truth (human/agent split documented)
- [ ] #6 Mac dev loop section explains scripts/dev-run.cjs branches: Mac → bash scripts/dev-run.sh, Windows → pnpm vendor:natives && pnpm tauri dev --config src-tauri/tauri.dev.conf.json
<!-- AC:END -->
