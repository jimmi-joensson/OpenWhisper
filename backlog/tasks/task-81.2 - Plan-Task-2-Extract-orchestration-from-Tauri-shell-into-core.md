---
id: TASK-81.2
title: 'Plan Task 2: Extract orchestration from Tauri shell into core/'
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
For each (O) and (M) symbol from Task 1's audit: move orchestration into core/, leave platform glue in shell. Likely candidates: media-pause/resume gate, settings store, behavior gating logic.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Every (O) symbol from audit doc 2 lives in core/
- [ ] #2 Every (M) symbol split: orchestration in core, platform glue in shell
- [x] #3 cargo check --workspace clean; cargo build -p openwhisper-core --features tauri clean
- [x] #4 pnpm test:ui green from apps/tauri/; no behavior regression in dictation flow
- [x] #5 core::diagnostics module exists in core/ with RecognizerInfo, DiagnosticsReadout, placeholder CrashDumpReader trait + CrashDump struct
- [x] #6 cargo build -p openwhisper-core --features macos-shell stays clean (SwiftUI shell isn't broken by the extraction)
- [ ] #7 apps/tauri/src-tauri/src/lib.rs reduction is explainable by audit doc 2's extraction list — no hard line target
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Three of five canonical commits landed:

- **Commit A — `core::media_gate`** (`cc77cd9`). Trait + PauseDiagnostic + MediaGateState + pause/take_paused_flag. Mac/Win impls now `impl openwhisper_core::media_gate::MediaController`; Mac's last_pause_diagnostic / probe_authorization fold into trait method overrides. Shell PAUSED_BY_US static gone. 6 new tests.
- **Commit B — `core::settings`** (`0cb736f`). Entire settings layer (~700 LOC of schema + JSON IO + 5 caches + 3 behavior atomics) moved. Tauri shell facade re-exports the schema types other shell modules consume via `crate::settings::*`; all `#[tauri::command]` wrappers now thin over core. behavior.rs drops its atomics, keeps `apply_collection_behavior` (NSPanel platform glue). 8 schema tests moved with the code (legacy JSON migration, clamp, defaults).
- **Commit D — `core::diagnostics`** (`2feb903`). RecognizerInfo + active_ep accessor on Recognizer trait + CrashDumpReader placeholder. 4 tests.

Deferred for follow-up:

- **Commit C — `core::dictation` augmentation.** The big extraction: do_toggle / do_cancel / audio_preview_start/stop / spawn_stop_pipeline / spawn_recognizer_warmup orchestration bodies + apply_fullscreen_state gating decision + phase_to_status status-string emission + DictationSnapshot::live_samples derivation. Needs a `ToggleEnv` trait shape decision (mic-auth gate, media-gate handle, spawn shim). Skipped this session because the CLI doesn't consume any of these — better to validate the trait shape against a real downstream caller (TASK-78 crash inspector? Linux contributor?) than guess now. AC #1, #2, #7 stay open until C lands.
- **Commit E — audio device-state shaping.** Move `AudioDeviceState` struct + `compute_audio_device_state` body into `core::audio`. Small.

Verification (after Commits A+B+D + CLI work): cargo check clean across default / tauri / macos-shell flavors; cargo test green (62 core lib tests); `pnpm test:ui` 87/87 passes after Commit B.
<!-- SECTION:NOTES:END -->
