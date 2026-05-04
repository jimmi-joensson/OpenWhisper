---
id: TASK-82
title: PR-gate CI workflow
status: To Do
assignee: []
created_date: '2026-05-04 15:43'
updated_date: '2026-05-04 15:47'
labels: []
milestone: m-1
dependencies: []
documentation:
  - backlog/docs/specs/doc-26 - PR-gate-CI-workflow-—-design.md
  - backlog/docs/plans/doc-27 - PR-gate-CI-workflow-—-implementation-plan.md
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
GitHub Actions workflow that runs on every PR and gates merge on Rust + frontend correctness. Mac + Windows for Rust gate; Mac only for frontend/Playwright gate (Tauri WebKit makes Win Playwright too heavy for v1). Stays lean during private-repo phase to conserve the 2000-min/month free tier; full matrix breathes after public-flip in TASK-NEW-5.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 ci.yml exists at .github/workflows/ci.yml; runs on pull_request to main + push to main + workflow_dispatch
- [ ] #2 Rust gate (cargo fmt --check, cargo clippy -D warnings, cargo test) green on macos-latest and windows-latest
- [ ] #3 Frontend gate (pnpm typecheck via tsc --noEmit + pnpm test:ui Playwright) green on macos-latest
- [ ] #4 Cache strategy reduces cold-run time: cargo registry, target/, pnpm store, ~/.cache/openwhisper/onnxruntime/, Playwright browsers
- [ ] #5 docs/maintainer/branch-protection.md documents the GitHub UI settings the maintainer must apply once public
<!-- AC:END -->
