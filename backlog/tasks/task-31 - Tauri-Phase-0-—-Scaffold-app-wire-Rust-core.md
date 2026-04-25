---
id: TASK-31
title: Tauri Phase 0 — Scaffold app + wire Rust core
status: Done
assignee: []
created_date: '2026-04-24 22:07'
updated_date: '2026-04-24 22:15'
labels:
  - tauri
  - phase-0
  - cross-platform
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Stand up apps/tauri/ as a Tauri 2 + React + TypeScript + Tailwind + shadcn/ui app. Link the existing openwhisper-core Rust crate as a Cargo path dependency with a "tauri" feature flag (see docs/tauri-port-handover.md §7).

Before scaffolding: feature-gate `swift-bridge` + `swift-bridge-build` in core/Cargo.toml behind a `macos-shell` feature so the Tauri target doesn't compile Swift FFI. Default features stay backward-compatible for the shipped Mac SwiftUI app.

Add `[profile.dev.package.openwhisper-core] opt-level = 3` to the workspace Cargo.toml so core builds release even in Tauri dev (rubato sinc resample is 50–120× slower in debug).

Smoke test only — no UI port, no recognizer, no pill window yet.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 apps/tauri/ scaffolded via pnpm create tauri-app (Tauri 2, React+TS template)
- [ ] #2 Tailwind + shadcn/ui base installed; Button component vendored under components/ui/
- [ ] #3 core/Cargo.toml: swift-bridge + swift-bridge-build behind 'macos-shell' feature; Mac SwiftUI build still green
- [ ] #4 apps/tauri/src-tauri/Cargo.toml depends on ../../core with features = ['tauri'], default-features = false
- [ ] #5 Workspace dev profile overrides openwhisper-core to opt-level=3
- [ ] #6 Main window renders and displays core_version() returned via a #[tauri::command]
- [ ] #7 pnpm tauri dev works on macOS; building from Windows deferred to Phase 1
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Phase 0 scaffold complete. apps/tauri/ (Tauri 2 + React + TS + Tailwind 4 + vendored shadcn-style Button) wired to openwhisper-core via Cargo path dep. swift-bridge + swift-bridge-build feature-gated behind `macos-shell` (default on); Tauri crate uses `default-features = false, features = ["tauri"]`. Workspace `[profile.dev.package.openwhisper-core] opt-level = 3` in root Cargo.toml. Smoke test: #[tauri::command] core_version() + invoke() in App.tsx. Verified: `cargo check -p openwhisper-tauri`, `cargo check -p openwhisper-core` (default features), `cargo build -p openwhisper-core` (regenerates apps/macos/Generated/), `pnpm build` (tsc + vite). pnpm tauri dev not GUI-verified in this session.
<!-- SECTION:FINAL_SUMMARY:END -->
