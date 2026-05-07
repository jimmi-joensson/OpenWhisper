---
id: TASK-62.6
title: 'Plan Task 6: Wrap Parakeet recognizer in ModelHandle (Windows)'
status: In Review
assignee: []
created_date: '2026-04-30 22:25'
updated_date: '2026-05-07 00:00'
labels:
  - 62-impl
dependencies: []
parent_task_id: TASK-62
ordinal: 9000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 OrtParakeet releases its ort::Session on Drop (verified or added)
- [ ] #2 After idle release, RSS drops by an observable amount on Windows — **awaiting Windows box manual smoke**
- [ ] #3 cargo check clean on x86_64-pc-windows-msvc — **partially de-risked via cross-compile from Mac to x86_64-pc-windows-gnu (clean); msvc final-check awaits Windows box**
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
- Commit: e484b36.
- ENGINE wrapping (the actual Mac path of TASK-62.5) automatically covers the Win path too — `engine()` in `core::recognizer::mod.rs` is platform-agnostic; `default_backend()` returns `OrtParakeet` on non-macOS targets. No Win-only refactor of `mod.rs` was needed.
- `OrtParakeet` Drop audit: `Sessions { encoder, decoder, joiner, ... }` already auto-Drops via field iteration; `ort::Session`'s own Drop releases the underlying ORT session. Implicit was sufficient. Added an **explicit** Drop on `OrtParakeet` anyway (a) so the manual Win smoke (AC #2) has a deterministic `eprintln` to wait for, (b) so the resource-release intent is unmistakable to a reader debugging "model still resident after unload". Fires only when sessions were Some — idempotent under repeated drops if any.
- Cross-compile from this Mac box to `x86_64-pc-windows-gnu` (the Win dev-box's toolchain per `core/Cargo.toml` ort `load-dynamic` rationale) is clean — the Drop addition + the broader 62.5 wrap compile against the Win path. `cargo check -p openwhisper-core --features tauri --target x86_64-pc-windows-gnu` finished without warnings. The final `x86_64-pc-windows-msvc` check still waits for the Windows box; no Mac-side toolchain installs MSVC link.exe.
- AC #2 (RSS drops on idle release) requires the Windows box and a real Parakeet load — not testable in CI or from Mac. Manual repro per spec: launch Tauri dev build on Win, dictate, watch RSS via `cli memory` (or Diagnostics → Memory once TASK-62.8 lands), wait 6 min, observe RSS drops by ~the model size (a few hundred MB). The new `[recognizer/ort] releasing sessions` eprintln in OrtParakeet::Drop signals the moment the timer fires.
- Plan risk note kept honest: `ort::Session::drop()` historically finicky in some `ort` versions when the runtime is unloaded mid-process. We don't unload the `ORT_INIT` global runtime between recognizer instances — only the sessions go. That's the supported path.
<!-- SECTION:NOTES:END -->
