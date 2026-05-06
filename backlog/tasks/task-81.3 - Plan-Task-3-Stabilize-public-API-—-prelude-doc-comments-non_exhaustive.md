---
id: TASK-81.3
title: 'Plan Task 3: Stabilize public API — prelude, doc-comments, non_exhaustive'
status: In Progress
assignee: []
created_date: '2026-05-04 15:09'
updated_date: '2026-05-06'
labels:
  - 81-impl
milestone: m-1
dependencies: []
parent_task_id: TASK-81
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Treat the post-extraction public surface as a real API: prelude module, doc-comments on every public item in prelude scope, #[non_exhaustive] discipline on enums and future-extensible structs.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 core::prelude module exists and exports the canonical types named in the spec
- [ ] #2 Every pub item in prelude-exported modules has a doc-comment explaining return type and failure modes
- [ ] #3 cargo doc --no-deps -p openwhisper-core renders without warnings on prelude items
- [x] #4 cargo build --workspace clean
- [ ] #5 Every named pub enum + struct (Phase, Toggle, SelectedDeviceStatus at audio.rs:609, TranscribeResult at recognizer/mod.rs:44, Snapshot, RecognizerInfo, DiagnosticsReadout, CrashDump) is #[non_exhaustive] or has comment citing concrete reason
- [x] #6 cargo build -p openwhisper-core --features macos-shell still clean — prelude reshuffle does not break SwiftUI shell
- [ ] #7 core/src/prelude.rs header comment documents that prelude is for default + tauri features; macos-shell uses per-module FFI signatures
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Prelude landed in commit `febabee`. Re-exports the canonical types per the doc-37 sketch from audio / diagnostics / dictation / media_gate / settings / stats / store / transcript; recognizer types gated behind `feature = "recognizer"`. cargo check clean across default / tauri / macos-shell.

Items added with `#[non_exhaustive]` along the way: `media_gate::PauseDiagnostic`, `diagnostics::RecognizerInfo`, `diagnostics::CrashDump`, `diagnostics::CrashId`, `diagnostics::ReadError`. Each ships with a `::new` ctor where struct expressions would otherwise be blocked from outside the crate.

Deferred for follow-up (AC #2, #3, #5, #7):

- Sweep the doc-comment punch list from doc-37 — every remaining undocumented `pub fn` / `pub struct` / `pub enum` in prelude-exported modules, plus `#![warn(missing_docs)]` enforcement on `core/src/lib.rs`.
- `#[non_exhaustive]` on the remaining items in the audit checklist: `audio::AudioDeviceInfo`, `audio::SelectedDeviceStatus`, `dictation::DictationSnapshot`, `transcript::FillerLang`, `recognizer::TranscribeResult`, `recognizer::mel::MelExtractor`, `recognizer::download::ModelPaths`, `stats::StatsSummary`, `store::StoreError`. Add the planned `Phase` / `ToggleAction` enums when extracting from the `pub const u32` constants.
- `core/src/prelude.rs` header comment about feature-gating (AC #7) — currently the prelude compiles under macos-shell; re-validate intent before committing to the AC's wording.
<!-- SECTION:NOTES:END -->
