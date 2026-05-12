---
id: TASK-81.11
title: 'Plan Task 11: Doc-comment sweep + #![warn(missing_docs)] enforcement'
status: To Do
assignee: []
created_date: '2026-05-12'
updated_date: '2026-05-12'
labels:
  - 81-impl
milestone: m-1
dependencies: []
parent_task_id: TASK-81
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Close out the doc-comment side of TASK-81.3 — the audit-flagged sweep of every undocumented `pub fn` / `pub struct` / `pub enum` in prelude-exported modules, plus `#![warn(missing_docs)]` enforcement on `core/src/lib.rs` so future drift is caught at compile time.

Split out of TASK-81.3 because the structural work there (`#[non_exhaustive]` sweep, zero cargo-doc warnings, prelude feature-gate header) had architectural taste — extracting trait shapes, choosing what to gate, etc. — while this remaining piece is bulk prose-authoring across ~160 pub items. Different shape, different review rhythm.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `RUSTFLAGS="-W missing_docs" cargo check -p openwhisper-core --features tauri` produces zero warnings
- [ ] #2 Same under `--features macos-shell`
- [ ] #3 `#![warn(missing_docs)]` set on `core/src/lib.rs` so subsequent drift is a compile-time signal, not an audit dependency
- [ ] #4 Doc-comments explain return type and failure modes for every prelude-exported `pub fn`; structs document field semantics; enums document each variant's meaning
- [ ] #5 `cargo doc --no-deps -p openwhisper-core --features tauri` still renders without warnings (already green after TASK-81.3, must stay green)
<!-- AC:END -->

## Implementation Plan
<!-- SECTION:PLAN:BEGIN -->
1. Audit the 161 missing-docs warnings under the tauri flavor; group by module so the sweep can land in module-sized commits.
2. Author doc-comments per module, biasing terse: return type + failure mode + non-obvious invariant. Avoid restating the function name.
3. Verify each module is clean before moving on; one commit per module is fine.
4. Add `#![warn(missing_docs)]` to `core/src/lib.rs` as the final commit so the lint stays on going forward.
5. Re-run `cargo doc` to confirm no new warnings.
<!-- SECTION:PLAN:END -->

## Out of scope

- `Phase` / `ToggleAction` enum extraction from the `pub const u32` constants — the audit named this as "Task 3 *will* extract it" but it's a bigger u32-to-enum refactor with FFI signature implications. Spin out separately if/when it bites.
- Private items in the same modules — `missing_docs` only fires on `pub`, which is the right scope. Internal helpers don't need this rigor.

## Notes
<!-- SECTION:NOTES:BEGIN -->
Parent task TASK-81.3 ships at Done with this AC retired and pointed at TASK-81.11. The 161-item count is the warning count under `--features tauri` at the end of the 2026-05-12 cleanup session — re-measure before starting to confirm it hasn't drifted.
<!-- SECTION:NOTES:END -->
