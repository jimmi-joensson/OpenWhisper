---
id: TASK-81
title: Library API audit + headless CLI
status: To Do
assignee: []
created_date: '2026-05-04 15:05'
updated_date: '2026-05-04 15:11'
labels: []
milestone: m-1
dependencies: []
documentation:
  - backlog/docs/specs/doc-24 - Library-API-audit-and-headless-CLI-—-design.md
  - >-
    backlog/docs/plans/doc-25 -
    Library-API-audit-and-headless-CLI-—-implementation-plan.md
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Lift core/'s public Rust API to be an ergonomic library consumed by both a new cli/ workspace member (Tailscale cmd/tailscale model) and the existing apps/tauri/src-tauri Tauri shell. By construction: CLI feature surface = UI feature surface = library surface.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 core/ public API audited; orchestration leaks from apps/tauri/src-tauri identified and migrated back into core/
- [ ] #2 Public API stabilized with prelude, doc-comments on every pub item, and a clean shape Tauri + CLI + tests all consume
- [ ] #3 New cli/ workspace member exists with subcommands transcribe / enumerate-devices / recognizer-info / crash-dump (stub for crash-dump until TASK-78 lands)
- [ ] #4 cli transcribe runs end-to-end on Mac (FluidAudio) and Windows (sherpa-onnx via ort), emitting transcript text to stdout
- [ ] #5 CI smoke runs cli transcribe against a bundled sample WAV and asserts non-empty output
- [ ] #6 apps/tauri/src-tauri Tauri commands refactored to one-liners over the public library API; Playwright suite (apps/tauri/tests/*.spec.ts) still passes
<!-- AC:END -->
