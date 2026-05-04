---
id: TASK-82.2
title: 'Plan Task 2: Rust gate — cargo fmt, clippy, cargo test on Mac + Win'
status: To Do
assignee: []
created_date: '2026-05-04 15:47'
updated_date: '2026-05-04 15:51'
labels:
  - 82-impl
milestone: m-1
dependencies: []
parent_task_id: TASK-82
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Fill in rust-gate-mac and rust-gate-win with the actual Rust gates. Both jobs share the same step list (toolchain → ort provisioning → fmt → clippy → test). Uses --features tauri (covers recognizer baseline); no --all-features (DirectML/CUDA would fail).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 rust-gate-win runs the same gates and passes on clean main
- [ ] #2 Deliberate cargo fmt violation pushed to a PR turns rust-gate-mac and rust-gate-win red
- [ ] #3 Deliberate clippy warning turns the corresponding job red
- [ ] #4 TASK-81.9 CLI smoke (once landed) executes via cargo test --workspace on both runners
- [ ] #5 rust-gate-mac runs cargo fmt --all --check, clippy --workspace --exclude bench-sherpa --no-default-features --features tauri -- -D warnings, and cargo test --workspace --exclude bench-sherpa --no-default-features --features tauri — all green on clean main
- [ ] #6 rust-gate-win runs the identical flag set and passes — swift-bridge excluded by --no-default-features, bench-sherpa CUDA libs never pulled
- [ ] #7 ort provisioning step uses pnpm --dir apps/tauri setup:ort (only package.json in repo lives at apps/tauri/)
- [ ] #8 pnpm/action-setup pinned to v4 with version 10 (matches dev box)
<!-- AC:END -->
