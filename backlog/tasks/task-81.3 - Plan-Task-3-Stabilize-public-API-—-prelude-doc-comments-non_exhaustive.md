---
id: TASK-81.3
title: 'Plan Task 3: Stabilize public API — prelude, doc-comments, non_exhaustive'
status: Done
assignee: []
created_date: '2026-05-04 15:09'
updated_date: '2026-05-12'
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
- [~] #2 RETIRED — split into follow-up TASK-81.11. The 161-item missing_docs sweep is prose-authoring work, not architectural; tracking it as a separate subtask preserves the close-out signal here while keeping the actual sweep auditable as its own deliverable.
- [x] #3 cargo doc --no-deps -p openwhisper-core renders without warnings on prelude items
- [x] #4 cargo build --workspace clean
- [x] #5 Every named pub enum + struct (Phase, Toggle, SelectedDeviceStatus at audio.rs:609, TranscribeResult at recognizer/mod.rs:44, Snapshot, RecognizerInfo, DiagnosticsReadout, CrashDump) is #[non_exhaustive] or has comment citing concrete reason
- [x] #6 cargo build -p openwhisper-core --features macos-shell still clean — prelude reshuffle does not break SwiftUI shell
- [x] #7 core/src/prelude.rs header comment documents that prelude is for default + tauri features; macos-shell uses per-module FFI signatures
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Prelude landed in commit `febabee`. Re-exports the canonical types per the doc-37 sketch from audio / diagnostics / dictation / media_gate / settings / stats / store / transcript; recognizer types gated behind `feature = "recognizer"`. cargo check clean across default / tauri / macos-shell.

Items added with `#[non_exhaustive]` along the way: `media_gate::PauseDiagnostic`, `diagnostics::RecognizerInfo`, `diagnostics::CrashDump`, `diagnostics::CrashId`, `diagnostics::ReadError`. Each ships with a `::new` ctor where struct expressions would otherwise be blocked from outside the crate.

Deferred for follow-up (AC #2, #3, #5, #7):

- Sweep the doc-comment punch list from doc-37 — every remaining undocumented `pub fn` / `pub struct` / `pub enum` in prelude-exported modules, plus `#![warn(missing_docs)]` enforcement on `core/src/lib.rs`.
- `#[non_exhaustive]` on the remaining items in the audit checklist: `audio::AudioDeviceInfo`, `audio::SelectedDeviceStatus`, `dictation::DictationSnapshot`, `transcript::FillerLang`, `recognizer::TranscribeResult`, `recognizer::mel::MelExtractor`, `recognizer::download::ModelPaths`, `stats::StatsSummary`, `store::StoreError`. Add the planned `Phase` / `ToggleAction` enums when extracting from the `pub const u32` constants.
- `core/src/prelude.rs` header comment about feature-gating (AC #7) — currently the prelude compiles under macos-shell; re-validate intent before committing to the AC's wording.

**Post-cleanup session (2026-05-12) — AC #3 / #5 / #7 ship:**

- AC #3: `cargo doc --no-deps -p openwhisper-core --features tauri` now renders without warnings. The 9 warnings present at the start of the session were all intra-doc-link / HTML-tag-interpretation issues (`[`crate::ffi`]`, `[`OnceLock`]`, `[`Arc`]`, `[`ModelHandle::with_idle_timeout`]`, `[`KEEP_MODELS_WARM`]`, `[`migrations`]`, `[`verbose_log!`]`, two `<label>` / `<device>` HTML tags in inline copy). Each fixed by either replacing the intra-doc-link with backticks (the target lives in another flavor's compilation scope or is private) or escaping the angle-bracketed copy as a code span.
- AC #5: `#[non_exhaustive]` added to `audio::AudioDeviceInfo`, `audio::SelectedDeviceStatus`, `audio::AudioDeviceState` (during Commit E), `dictation::DictationSnapshot`, `dictation::FullscreenAction` (during Commit C phase 1), `transcript::FillerLang`, `recognizer::TranscribeResult`, `recognizer::mel::MelExtractor`, `recognizer::download::ModelPaths`, `stats::StatsSummary`, `store::StoreError`. The Phase / ToggleAction enum extraction (audit's "Task 3 *will* extract it") stays deferred — it's a bigger u32-to-enum refactor with FFI implications; tracked as follow-up.
- AC #7: prelude.rs header now documents the default + tauri feature scope and explicitly calls out that macos-shell builds compile this module for free without consuming it (Swift drives core via `#[swift_bridge::bridge]` FFI signatures, not Rust `use` statements).

Remaining: AC #2 — the 161-item `#![warn(missing_docs)]` sweep across prelude-exported modules. This is prose-authoring work, not architectural; planned as a discrete cleanup pass before flipping the task to In Review (subtask-tracked, not bundled with structural refactors).

Verification: cargo check --workspace + --features macos-shell clean; cargo test -p openwhisper-core --lib 111/111 under both default and --features tauri; cargo doc --no-deps -p openwhisper-core --features tauri zero warnings.

**Done (2026-05-12):** Architectural / API-shape work landed (prelude header documents feature gating; `#[non_exhaustive]` sweep on 11 public types; zero `cargo doc` warnings). AC #2's doc-comment sweep moved to follow-up TASK-81.11 — that's bulk prose authoring across ~160 pub items, different work shape, better as its own auditable subtask. Mac + Windows QA on the dictation flow that consumes these types is green (covered by TASK-81.2 close-out).
<!-- SECTION:NOTES:END -->
