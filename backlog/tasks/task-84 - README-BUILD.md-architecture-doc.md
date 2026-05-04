---
id: TASK-84
title: README + BUILD.md + architecture doc
status: To Do
assignee: []
created_date: '2026-05-04 16:18'
updated_date: '2026-05-04 16:22'
labels: []
milestone: m-1
dependencies: []
documentation:
  - backlog/docs/specs/doc-30 - README-BUILD.md-architecture-doc-—-design.md
  - >-
    backlog/docs/plans/doc-31 -
    README-BUILD.md-architecture-doc-—-implementation-plan.md
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Bring the user-facing entry doc (README), the contributor-facing build doc (BUILD.md, new), and the architecture rationale doc (docs/contributing/architecture.md, new) up to v1 polish. Externalize the openwhisper-orchestration-in-rust skill rule for public consumption. Lands in parallel with TASK-83. Stays under 'OpenWhisper' name; rename in TASK-NEW-5.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 README rewrite ships: pitch sharpened, install/build sections point at correct downstream docs, no stale claims (e.g. 'no binaries yet' when TASK-46 has shipped)
- [ ] #2 Badges added to README: CI status (after TASK-82 lands), license, platforms
- [ ] #3 BUILD.md committed at repo root, covering contributor dev loop end-to-end (clone → install deps → scripts/dev-run.sh → cross-platform gotchas link → Backlog task model)
- [ ] #4 docs/contributing/architecture.md committed externalizing the orchestration-in-rust rule + recognizer trait + Tauri/core split — readable without skill-loader access
- [ ] #5 Demo GIF embedded in README OR placeholder text shipped (don't block milestone on visual freeze)
<!-- AC:END -->
