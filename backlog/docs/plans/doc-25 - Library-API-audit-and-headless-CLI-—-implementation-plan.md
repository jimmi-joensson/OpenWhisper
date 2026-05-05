---
id: doc-25
title: Library API audit and headless CLI — implementation plan
type: plan
created_date: '2026-05-04 15:06'
---

# Library API audit + headless CLI — implementation plan

**Backlog parent:** TASK-81
**Spec:** `backlog/docs/specs/doc-24 - Library-API-audit-and-headless-CLI-—-design.md`
**Milestone:** m-1 — v1.0 public release readiness

## Ordering

Tasks 1 → 2 → 3 are sequential — each unblocks the next.
Tasks 4 → 5 → 6 → 7 → 8 build the CLI on top of the stabilized API; 5 depends on 4, but 6/7/8 can run in parallel after 4.
Task 9 (CI smoke) depends on Task 5 landing.
Task 10 (Tauri refactor) depends on Task 3; can land any time after.

```
1 ── 2 ── 3 ──┬── 4 ── 5 ──┬── 9
              │      ├── 6 │
              │      ├── 7 │
              │      └── 8 │
              └────── 10 ──┘
```

## Task 1: Audit `core/` public API + identify shell orchestration leaks

Read every `pub` in `core/src/` and every `#[tauri::command]` in `apps/tauri/src-tauri/src/`. Produce two artifacts as commits in `backlog/docs/`:

- `backlog/docs/audits/2026-05-04-core-public-api.md` — every `pub fn`, `pub struct`, `pub enum`, `pub trait` in `core/`, grouped by capability (capture / dictation / transcribe / device-enum / settings / diagnostics). Note doc-comment status, ergonomics notes (e.g. "returns `Vec<f32>` — should be `&[f32]` to avoid alloc on hot path").
- `backlog/docs/audits/2026-05-04-tauri-orchestration-leaks.md` — for each Tauri command, and **every helper in `apps/tauri/src-tauri/src/lib.rs` that mutates global state or makes platform API calls** (the 5-line heuristic was wrong — small pure glue like `phase_to_status` doesn't matter, but a 4-line function holding a global mutex does). Classify as: `(P)` pure platform glue (stays in shell), `(O)` orchestration (move to core), `(M)` mixed (split). Cite line numbers. Categories follow the `openwhisper-orchestration-in-rust` skill rule — state machines, phase transitions, gating logic, status strings → `(O)`. Specifically inspect:
  - `pause_audio_for_recording` / `resume_audio_after_recording` (lib.rs:56-88)
  - `PAUSED_BY_US` invariant (lib.rs:40)
  - `MEDIA_CONTROLLER` global (lib.rs:34)
  - `behavior::pause_audio_during_dictation()` and the rest of `behavior.rs` (170 lines)
  - the dictation-phase ↔ media-controller coupling
  - `settings/mod.rs` — does it duplicate any state machine that should live in core?

Use `backlog doc create "..." -p audits -t spec` (audit subdir is fine — Backlog accepts arbitrary subdirs under `docs/`).

**Outcomes:**
- Audit doc 1 committed; lists every `pub` item with capability grouping.
- Audit doc 2 committed; every Tauri shell symbol that mutates global state or calls platform APIs is classified P/O/M with line-number cite.
- Concrete list of orchestration leaks to extract in Task 2 (named in audit doc 2 as a checklist).

**Verification:** PR reviewer confirms every `pub` in `core/src/` appears in audit doc 1, and audit doc 2 covers every shell helper that touches global state or platform APIs (not the 5-line pure-glue helpers).

## Task 2: Extract orchestration from Tauri shell into `core/`

For each `(O)` and `(M)` symbol from Task 1's audit doc 2: move the orchestration into `core/`, leaving the platform glue behind. **This task is at least 4 commits** — likely more once the audit lands. Plan body lists the canonical extraction commits below; Backlog AC #1 covers the union outcome:

1. **Commit A — `core::media_gate`.** Pause/resume gate logic + `PAUSED_BY_US` idempotency invariant. Shell's `pause_audio_for_recording` becomes:
   ```rust
   fn pause_audio_for_recording(controller: &impl MediaController) {
       core::media_gate::pause(controller, &MEDIA_GATE_STATE);
   }
   ```
   `MediaController` trait stays in shell with platform impls.
2. **Commit B — `core::settings`.** Schema, atomic-flag handling, `SettingsStore`. Tauri shell calls `settings::load()` / `settings::save()` and exposes them as commands.
3. **Commit C — `core::dictation` augmentation.** Move any `behavior.rs` functions that gate dictation phase transitions next to the existing state machine.
4. **Commit D — `core::diagnostics` module creation.** New module with `RecognizerInfo`, `DiagnosticsReadout`, and a placeholder `pub trait CrashDumpReader { fn list(&self) -> Vec<CrashId>; fn read(&self, id: CrashId) -> Result<CrashDump, ReadError>; }` that TASK-78 will implement against. The `CrashDump` struct can be empty for now (`pub struct CrashDump { /* TASK-78 fills */ }` with `#[non_exhaustive]`). This unblocks Task 7 (`recognizer-info`) and Task 8 (`crash-dump` stub).
5. **Commit E onward — remaining (O) and (M) extractions** as audit doc 2 dictates.

Leave in the shell: NSPanel ops, hotkey hook (mac.rs / windows.rs), fullscreen detection, tray menu, TCC reset, focus-window monitor — all platform glue, no CLI analog.

Run `cargo check --workspace` and `cargo build -p openwhisper-core --features tauri` after each commit.

**Outcomes:**
- Every `(O)` symbol from audit doc 2 now lives in `core/`.
- Every `(M)` symbol split: orchestration in core, platform glue in shell.
- `core::diagnostics` module exists with `RecognizerInfo`, `DiagnosticsReadout`, and a placeholder `CrashDumpReader` trait + `CrashDump` struct (concrete `CrashDump` fields land in TASK-78).
- `cargo check --workspace` clean. `cargo build -p openwhisper-core --features tauri` clean. `cargo build -p openwhisper-core --features macos-shell` also clean (the SwiftUI shell stays buildable through this work).
- `apps/tauri/src-tauri/src/lib.rs` is meaningfully shorter — line-reduction roughly proportional to the (O) symbols audit doc 2 listed. **No hard line target**: if the audit honestly classifies most large blocks as `(P)`, the shell stays large; that's correct behavior.
- Existing Playwright suite (`pnpm test:ui` from `apps/tauri/`) passes — no behavior regression.

**Verification:** `cargo check`, `cargo build -p openwhisper-core --features tauri`, `cargo build -p openwhisper-core --features macos-shell`, `cargo test -p openwhisper-core`, `pnpm test:ui`. Manual smoke of the dictation flow on Mac.

## Task 3: Stabilize public API — `prelude`, doc-comments, `#[non_exhaustive]`

Now that core has the full feature set, treat the public surface as a real API:

- Add `core/src/prelude.rs` exporting the canonical types per the spec (`AudioCapture`, `DictationEngine`, `Phase`, `Snapshot`, `Recognizer`, `RecognizerInfo`, `TranscriptFilter`, `Settings`, `SettingsStore`, `DiagnosticsReadout`, `CrashDumpReader`).
- `pub use prelude::*;` is opt-in by consumers; library still exposes per-module paths for granular use.
- **Feature-gating note:** prelude is in scope for the default and `tauri` feature flavors. The `macos-shell` feature path (which compiles `swift_shims` and the `swift_bridge::bridge` mod) keeps using the per-module FFI signatures — Swift consumers don't get the prelude. Document this in `core/src/prelude.rs` as a header comment, and add a `#[cfg(not(feature = "macos-shell"))]` gate if the prelude pulls in types Swift can't see.
- Doc-comments on every `pub fn` and `pub struct/enum` in modules covered by the prelude. Emphasis on *what the function returns and when it can fail*, not on *how it works internally*. Run `cargo doc --no-deps -p openwhisper-core` and skim the rendered output for gaps.
- `#[non_exhaustive]` on every `pub enum` and on `pub struct`s where future fields are likely. **Concrete list as of writing** (audit can add more): `dictation::Phase`, `dictation::Toggle`, `audio::SelectedDeviceStatus` (`audio.rs:609`), `recognizer::TranscribeResult` (`recognizer/mod.rs:44`), `Snapshot`, `RecognizerInfo`, `DiagnosticsReadout`, `CrashDump`. The escape hatch ("or has an explicit comment justifying why not") **must cite a concrete reason** — `// FFI-stable, locked by swift-bridge bindings`, `// exhaustive by language design (uninhabited)`, etc.
- `#![warn(missing_docs)]` on `core/src/lib.rs` to enforce going forward. Fix or `#[allow(missing_docs)]` on existing items as appropriate.

**Outcomes:**
- `core::prelude` module exists and exports every type called out in the spec.
- `cargo doc --no-deps -p openwhisper-core` renders without warnings on prelude items.
- `cargo build -p openwhisper-core --features macos-shell` still clean — the SwiftUI shell isn't broken by the prelude reshuffle.
- Every `pub` in prelude-exported types has a doc-comment.
- Every `pub enum` in core (and the named structs above) is `#[non_exhaustive]`, or has a comment citing a concrete reason for the exception.
- `cargo build --workspace` clean across the default, `tauri`, and `macos-shell` feature flavors.

**Verification:** `cargo doc --no-deps -p openwhisper-core 2>&1 | grep -i warn` is empty for prelude items. Reviewer confirms `#[non_exhaustive]` on every public enum and on the named structs. Reviewer confirms the `macos-shell` build is clean.

## Task 4: New `cli/` crate scaffold + clap parser

- Add `cli/` to root `Cargo.toml` workspace members (alongside `core`, `apps/tauri/src-tauri`, `scripts/bench/bench-sherpa`).
- `cli/Cargo.toml`: `[package] name = "openwhisper-cli"`, `[[bin]] name = "openwhisper"`, deps: `openwhisper-core` (path, features = ["recognizer"]), `clap = { version = "4", features = ["derive"] }`, `anyhow`, `serde_json`.
- `cli/src/main.rs`: clap derive enum with subcommands `Transcribe`, `EnumerateDevices`, `RecognizerInfo`, `CrashDump`. Empty handlers (`unimplemented!()`). Global `--json` flag.
- `cli/src/commands/{transcribe,enumerate_devices,recognizer_info,crash_dump}.rs` — one file per subcommand, each exposes `pub fn run(args: ..., json: bool) -> anyhow::Result<()>`.
- `cli --help` prints the subcommand list.

**Outcomes:**
- Workspace builds: `cargo build -p openwhisper-cli`.
- `cargo run -p openwhisper-cli -- --help` prints subcommand list.
- `cargo run -p openwhisper-cli -- transcribe --help`, `enumerate-devices --help`, etc. all succeed (unimplemented handlers, but parser shape works).
- No new dep introduced into `core/` or `apps/tauri/src-tauri/`.

**Verification:** `cargo build --workspace`. Manual `--help` runs.

## Task 5: `cli transcribe <wav>` end-to-end

Implement the transcribe handler against the stabilized library API. **Land Mac first, Windows second** — Mac uses the well-trodden FluidAudio path (the existing Tauri dictation flow already exercises it). Windows is the higher-risk leg: loading sherpa-onnx + `ort` outside the Tauri runtime context is unproven; we may discover the recognizer construction needs Tauri-state plumbing that's hard to recreate in a headless context. Two commits:

**Commit 5a — Mac path.**
- Read WAV with `hound` (already a dev-dep on core; promote to `cli` direct dep).
- Validate: 16 kHz mono f32. If not, resample/downmix using the same path the live capture uses (or error out clearly).
- Construct `Recognizer` via `core::prelude` (FluidAudio impl on Mac).
- Run inference, print transcript text to stdout. With `--json`, print `{ "text": "...", "confidence": 0.93, "duration_ms": 1234 }`.
- Errors to stderr; exit code non-zero on failure.

**Commit 5b — Windows path.**
- Same handler, Windows `cfg`. Construct `Recognizer` via `core::prelude` (sherpa-onnx + `ort` impl).
- If construction needs Tauri-state plumbing currently, file a follow-up task; do not block 5a on the discovery.
- Verify with the same `--json` smoke against the same sample WAV.

**Outcomes:**
- `cargo run -p openwhisper-cli -- transcribe core/tests/fixtures/sample.wav` prints non-empty text on Mac.
- Same command on Windows runs sherpa-onnx and prints non-empty text.
- `--json` mode emits valid JSON parseable by `jq -e '.text | length > 0'`.
- No `unsafe`, no `unwrap` in the CLI handler; errors propagate via `anyhow::Result`.
- If Windows leg uncovers a private dep on Tauri state, a follow-up Backlog task is filed and 5b ships its mitigation.

**Verification:** Manual smoke with a known-good sample WAV on Mac (5a) and on Windows (5b). Compare output to a Tauri-shell transcription of the same audio (should be identical word stream — different post-processing flags noted in plan).

## Task 6: `cli enumerate-devices`

- Library call: `openwhisper_core::audio::enumerate_devices()` (already exists or extract from shell — should fall out of Task 2).
- Print one device per line: `<name>\t<id>\t<sample_rate>\t<is_default>\t<is_virtual>` (Mac filters virtual mics via coreaudio `kAudioDevicePropertyTransportType` per existing logic).
- `--json` emits an array of device objects.

**Outcomes:**
- `cli enumerate-devices` lists at least one device on Mac and Windows hosts.
- Default mic is flagged.
- Virtual mics (Teams, Zoom, BlackHole) filtered on Mac.
- `--json` output validates against a tiny inline schema.

**Verification:** Run on Mac with BlackHole installed; verify it's filtered. Run on Windows with default mic; verify it appears.

## Task 7: `cli recognizer-info`

**Depends on Task 2 Commit D** (`core::diagnostics` module exists with `RecognizerInfo`). If Task 2 doesn't land Commit D, Task 7 is blocked.

- Library call: `core::diagnostics::recognizer_info()` returns `RecognizerInfo { engine, model_path, version, ep }` (engine = "FluidAudio" / "Sherpa+ort"; ep = "ANE" / "DirectML" / "CPU"; version = parakeet model version).
- Print as a small table (label / value), or JSON with `--json`.

**Outcomes:**
- `cli recognizer-info` prints the active engine, model path, model version, EP on both Mac and Windows.
- The values match what the Tauri Diagnostics panel shows.

**Verification:** Cross-check output against Diagnostics panel (TASK-65 home pane already surfaces these).

## Task 8: `cli crash-dump` (stub against `CrashDumpReader` placeholder from Task 2)

**Depends on Task 2 Commit D** — the placeholder `pub trait CrashDumpReader` and `CrashDump` struct are introduced there, even though TASK-78 fills in the concrete file-format and reader impl later. This task wires the CLI subcommand against that trait surface so:
- The CLI compiles and the subcommand registers in `--help`.
- Once TASK-78 lands a concrete `FileBackedCrashDumpReader` impl, Task 8's handler swaps from "no reader available" to calling it without a CLI redesign.

Concretely:

- Subcommand surface exists with these flags: `--latest` / `--id <n>` / `--list`.
- Handler attempts to obtain a `CrashDumpReader` via `core::diagnostics::default_crash_reader()`. Until TASK-78 ships, that function returns `None` (or a no-op reader). Handler prints `crash reporting not yet enabled (TASK-78)` to stderr and exits 0 — so CI smoke doesn't break.
- Add a TODO in `cli/src/commands/crash_dump.rs` referencing TASK-78.

**Outcomes:**
- `cli crash-dump --help` lists `--latest` / `--id` / `--list`.
- Each flag, when invoked, prints the deferred-feature notice and exits 0.
- Code uses the `CrashDumpReader` trait shape — not a hand-rolled placeholder — so TASK-78 plugs in cleanly.
- TODO references TASK-78 explicitly.

**Verification:** Manual help + invocation. Reviewer confirms the CLI handler imports `core::diagnostics::CrashDumpReader` (or the `default_crash_reader()` accessor) — proving the contract is real, not informal.

## Task 9: CI smoke — `cli transcribe` against bundled WAV

- Add `cli/tests/fixtures/hello-world.wav` — pinned format **16 kHz mono i16 PCM WAV**, ~3 s, expected size ~96 KB. (16 kHz mono f32 would be ~190 KB and bust the size budget.) Recorded once, committed.
- Add `cli/tests/smoke.rs` — `assert_cmd` integration test: spawn `openwhisper transcribe ./tests/fixtures/hello-world.wav --json`, parse stdout, assert `.text` is non-empty and contains `"hello"` (case-insensitive).
- Wire into the workspace `cargo test`. CI workflow (TASK-NEW-2 / pillar 3) picks it up.

**Outcomes:**
- `cargo test -p openwhisper-cli` runs smoke.rs and passes on Mac (FluidAudio).
- Same test passes on Windows runner (sherpa-onnx + ort).
- Smoke fixture is committed and < 100 KB.

**Verification:** `cargo test -p openwhisper-cli` locally on Mac. Once CI workflow lands, verify on Windows runner via PR check.

## Task 10: Tauri commands cleanup pass

**This is a cleanup pass, not a second extraction wave.** Task 2 already moved orchestration into `core/`. Task 10 walks the resulting `#[tauri::command]` bodies and tightens anything that didn't get its turn during Task 2: rename for clarity, replace ad-hoc tuples with `core::prelude` types, delete dead code paths the extraction made unreachable.

If Task 10 finds non-trivial business logic still in the shell that wasn't on audit doc 2's list, that's a Task 2 miss — file it back as a follow-up subtask under TASK-81 rather than expanding Task 10's scope.

Walk every `#[tauri::command]` in `apps/tauri/src-tauri/src/`:

- If the command body is a thin call into `core::`, leave it.
- If it has remaining business logic the audit missed, file a follow-up; do not extract under Task 10.
- Where state needs threading (e.g. `MEDIA_CONTROLLER`), pass it explicitly as a parameter — Tauri commands take `tauri::State<...>` arguments cleanly.
- Run `pnpm test:ui` from `apps/tauri/` after each batch of refactors.

Target shape:

```rust
#[tauri::command]
fn dictation_toggle(state: State<'_, AppState>) -> Result<u32, String> {
    Ok(openwhisper_core::dictation::toggle())
}
```

**Outcomes:**
- Every `#[tauri::command]` body is a thin delegation, or has a code comment justifying remaining shell logic as platform glue.
- Any business logic discovered in the shell that wasn't on audit doc 2's list is filed as a follow-up subtask, not silently extracted.
- Playwright suite green: `pnpm test:ui` from `apps/tauri/`.
- Manual dictation smoke (record → transcribe → paste) passes on Mac.

**Verification:** `cargo build`, `pnpm test:ui`, manual dictation flow. Reviewer confirms no command body exceeds ~10 lines without a glue justification comment.

## Cross-task verification checklist

Before marking TASK-81 done:

- [ ] All 10 subtasks `Done` in Backlog.
- [ ] `cargo build --workspace` clean on Mac and Windows.
- [ ] `cargo test --workspace` green.
- [ ] `pnpm test:ui` green from `apps/tauri/`.
- [ ] Manual dictation flow (Mac): hotkey → record → release → text appears in target app.
- [ ] Manual dictation flow (Windows): same end-to-end smoke.
- [ ] `cli transcribe`, `enumerate-devices`, `recognizer-info` produce correct output on both platforms.
- [ ] `apps/tauri/src-tauri/src/lib.rs` is meaningfully shorter than 1081 lines, with the reduction explainable by audit doc 2's extraction list (no hard line target).
- [ ] No `pub fn` in `core/` consumed only by tests — every `pub` is exercised by either CLI or Tauri shell.
- [ ] `cargo build -p openwhisper-core --features macos-shell` clean — the SwiftUI shell still builds.
