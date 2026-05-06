---
id: doc-37
title: openwhisper-core public API audit (2026-05-06)
type: spec
created_date: '2026-05-06'
---

# openwhisper-core public API audit

**Backlog parent:** TASK-81.1 (Plan Task 1)
**Spec:** `backlog/docs/specs/doc-24 - Library-API-audit-and-headless-CLI-—-design.md`
**Plan:** `backlog/docs/plans/doc-25 - Library-API-audit-and-headless-CLI-—-implementation-plan.md`

Inventory of every `pub` item under `core/src/` as of `b3ab642` (`main`).
Feeds Task 2 (extraction) and Task 3 (prelude + `#[non_exhaustive]` discipline).

Doc-comment column: **D** = full doc-comment, **P** = partial / module-level
only, **N** = none.

`NE?` column: `Y` if `#[non_exhaustive]`, `n` otherwise (only listed for
`pub struct` / `pub enum`).

---

## Module map (lib.rs:1-9)

| pub item | kind | line | doc | gate |
|---|---|---|---|---|
| `audio` | mod | lib.rs:1 | P | always |
| `dictation` | mod | lib.rs:2 | P | always |
| `recognizer` | mod | lib.rs:5 | P | `feature = "recognizer"` |
| `stats` | mod | lib.rs:6 | P | always |
| `store` | mod | lib.rs:7 | P | always |
| `transcript` | mod | lib.rs:8 | P | always |
| `verbose` | mod | lib.rs:9 | D | always |
| `core_version()` | fn | lib.rs:48 | N | always |
| `swift_bridge::bridge` ffi mod | mod | lib.rs:13-46 | N | `feature = "macos-shell"` |
| `swift_shims` | mod | lib.rs:55-108 | P | `feature = "macos-shell"` |

`mod ffi_c` (lib.rs:3) is private but the symbols it contains are `pub
extern "C"` and ship in the staticlib for the Windows C# shell —
treated as a public surface in the **ffi-c** group below.

---

## capture (`core::audio`)

| pub item | kind | line | doc | NE? | notes |
|---|---|---|---|---|---|
| `AudioDeviceInfo` | struct | audio.rs:47 | P | n | three pub fields (`id`, `label`, `is_default`); should be `#[non_exhaustive]` to leave room for transport-type / sample-rate without a breaking change. |
| `AudioEngine` | struct | audio.rs:85 | N | n | engine handle; one private field (`ctrl_tx`). Doc-comment missing. |
| `AudioEngine::new()` | fn | audio.rs:112 | N | — | `Self`. Spawns worker thread; that side-effect is undocumented. |
| `AudioEngine::start()` | fn | audio.rs:127 | N | — | `Result<(), String>`. |
| `AudioEngine::start_preview()` | fn | audio.rs:131 | N | — | `Result<(), String>`. |
| `AudioEngine::stop()` | fn | audio.rs:135 | N | — | |
| `AudioEngine::drain()` | fn | audio.rs:139 | N | — | `Vec<f32>` — hot path, alloc per call. Could be `&mut Vec<f32>` or a borrow-into pattern; not blocking for v1 but flag for the prelude. |
| `AudioEngine::is_capturing()` | fn | audio.rs:143 | N | — | |
| `AudioEngine::is_previewing()` | fn | audio.rs:147 | N | — | |
| `audio_start_capture()` | fn | audio.rs:403 | D | — | `Result<(), String>`. Documented preview teardown side-effect. |
| `audio_stop_capture()` | fn | audio.rs:414 | N | — | |
| `audio_drain_samples()` | fn | audio.rs:422 | N | — | `Vec<f32>` — same hot-path note as `AudioEngine::drain()`. |
| `estimate_voiced_ms(samples, sample_rate)` | fn | audio.rs:454 | D | — | excellent doc-comment with rationale + units. |
| `audio_is_capturing()` | fn | audio.rs:591 | N | — | |
| `audio_preview_start()` | fn | audio.rs:599 | D | — | |
| `audio_preview_stop()` | fn | audio.rs:605 | N | — | |
| `audio_current_level()` | fn | audio.rs:815 | D | — | documented stale-stream behavior. |

### device-enum (subset of `core::audio`)

| pub item | kind | line | doc | NE? | notes |
|---|---|---|---|---|---|
| `audio_list_input_devices()` | fn | audio.rs:633 | D | — | `Vec<AudioDeviceInfo>`. Documents filter rules (virtual mics, probe-open). |
| `audio_set_selected_device_id(id)` | fn | audio.rs:752 | D | — | |
| `audio_get_selected_device_id()` | fn | audio.rs:758 | N | — | |
| `audio_default_input_label()` | fn | audio.rs:768 | D | — | |
| `SelectedDeviceStatus` | enum | audio.rs:773 | D | n | three variants — `Present`, `MissingFallbackToDefault`, `NoneSelectedUsingDefault`. **Add `#[non_exhaustive]`** before exposing — adding a `Disabled` / `PermissionDenied` variant in v1.x should not break consumers. |
| `audio_selected_device_status()` | fn | audio.rs:791 | D | — | |

---

## dictation (`core::dictation`)

| pub item | kind | line | doc | NE? | notes |
|---|---|---|---|---|---|
| `PHASE_IDLE` … `PHASE_ERROR` (6 consts) | const u32 | dictation.rs:26-31 | P | — | Module header documents the values; consts themselves uncommented. **Convert to a `pub enum Phase { Idle, Loading, Recording, Transcribing, Done, Error }` with `#[non_exhaustive]` and explicit `repr(u32)` for FFI** — the `pub const u32` shape is a swift-bridge artifact. The enum can serialize to u32 across FFI via a helper. |
| `TOGGLE_IGNORE`, `TOGGLE_BEGIN_RECORDING`, `TOGGLE_STOP_RECORDING` | const u32 | dictation.rs:34-36 | N | — | Same treatment — `pub enum ToggleAction`. |
| `DictationSnapshot` | struct | dictation.rs:95 | D | n | opaque-by-construction (all fields private, getters only) — design is sound, but **add `#[non_exhaustive]` for parity with the planned `Snapshot` rename** and to protect against new accessors being a breaking change. |
| `DictationSnapshot::phase()` ↓ `download_bytes_total()` (11 fns) | fn | dictation.rs:110-141 | N | — | All accessors clone `String` fields per call; consider `&str` returns post-prelude (less critical — UI ticks do clone anyway). |
| `is_recording()` | fn | dictation.rs:158 | D | — | documented lock-free mirror; safe to call from OS hooks. |
| `dictation_snapshot()` | fn | dictation.rs:168 | N | — | |
| `dictation_request_toggle()` | fn | dictation.rs:187 | N | — | returns `u32` (TOGGLE_*). After enum extraction returns `ToggleAction`. |
| `dictation_request_cancel()` | fn | dictation.rs:207 | N | — | |
| `dictation_mark_loading_model()` | fn | dictation.rs:228 | N | — | |
| `dictation_set_download_progress(done, total)` | fn | dictation.rs:244 | D | — | |
| `dictation_set_extract_progress(done, total)` | fn | dictation.rs:253 | D | — | |
| `dictation_mark_loaded()` | fn | dictation.rs:276 | D | — | |
| `dictation_mark_loading_session()` | fn | dictation.rs:289 | D | — | |
| `dictation_mark_capture_started()` | fn | dictation.rs:297 | N | — | |
| `dictation_mark_transcribing_pending()` | fn | dictation.rs:322 | D | — | excellent rationale (sinc resample on hotkey thread). |
| `dictation_set_voiced_ms(ms)` | fn | dictation.rs:337 | D | — | |
| `dictation_mark_capture_stopped(sample_count)` | fn | dictation.rs:343 | N | — | |
| `dictation_deliver_transcript(text, confidence)` | fn | dictation.rs:359 | N | — | invokes `INJECTOR` + `stats::record_dictation` as side-effects — both undocumented. |
| `Injector` | trait | dictation.rs:412 | D | — | `Send + Sync`, single method `inject(&self, text: &str)`. |
| `set_injector(injector)` | fn | dictation.rs:420 | D | — | |
| `dictation_deliver_error(message)` | fn | dictation.rs:424 | N | — | |

**Naming note for Task 3.** The `dictation_*` prefix is a swift-bridge
flat-namespace artifact (FFI symbols can't shadow each other across
modules). Inside Rust they read as repetitive: `dictation::dictation_snapshot()`.
The prelude sketch in doc-24 already plans to re-export these without the
prefix as `Snapshot::current()` / `Engine::request_toggle()` etc.; the
underlying free fns can stay as the FFI surface with shorter sigs added
on top.

---

## transcribe (`core::transcript`)

| pub item | kind | line | doc | NE? | notes |
|---|---|---|---|---|---|
| `FillerLang` | enum | transcript.rs:26 | N | n | `En` / `Da`. **Add `#[non_exhaustive]`** — multilingual support (TASK-15/16) will add `Es`, `De`, etc. |
| `detect_lang(text)` | fn | transcript.rs:88 | N | — | `FillerLang`. |
| `process(text)` | fn | transcript.rs:97 | P | — | module header is the doc; fn itself bare. |

---

## recognizer (`core::recognizer`, feature-gated)

| pub item | kind | line | doc | NE? | notes |
|---|---|---|---|---|---|
| `FluidAudioBridge` (re-export) | struct | recognizer/mod.rs:38 | D | n | `cfg(target_os = "macos")`. Defined at `recognizer/fluidaudio.rs:30`; unit struct, no fields. |
| `OrtParakeet` (re-export) | struct | recognizer/mod.rs:40 | D | n | `cfg(not(target_os = "macos"))`. Defined at `recognizer/ort_parakeet.rs:48`; 4 private fields. |
| `TranscribeResult { text, confidence, elapsed_ms }` | struct | recognizer/mod.rs:44 | P | n | doc-comment on `confidence` only. **Add `#[non_exhaustive]`** — likely fields: `tokens`, `language`, `model_version`. |
| `Recognizer` | trait | recognizer/mod.rs:55 | D | — | `Send`. Two methods (`ensure_loaded`, `transcribe`). |
| `recognizer_ensure_loaded()` | fn | recognizer/mod.rs:78 | D | — | `Result<(), String>`. |
| `recognizer_transcribe(samples)` | fn | recognizer/mod.rs:86 | D | — | |
| **Mac sub-impl: `FluidAudioBridge::new()` / `default()`** | fn | recognizer/fluidaudio.rs:33-42 | N | — | unit ctors. |
| **Win sub-impl: `OrtParakeet::new()`** | fn | recognizer/ort_parakeet.rs:74 | N | — | |
| `OrtParakeet::selected_ep()` | fn | recognizer/ort_parakeet.rs:84 | D | — | `&str`. Used by bench harness. |
| `ModelPaths { encoder, decoder, joiner, tokens }` | struct | recognizer/download.rs:21 | N | n | four `pub: PathBuf` fields. Currently private to non-Mac builds. |
| `ensure_model()` | fn | recognizer/download.rs:31 | D | — | `Result<ModelPaths, String>`. |
| `EpChoice { eps, label }` | struct | recognizer/ep_probe.rs:33 | D | n | `Vec<ExecutionProviderDispatch>` exposes ort-internal type to consumers. **Hide behind a `Recognizer::info() -> RecognizerInfo` accessor in core::diagnostics; keep `EpChoice` private.** |
| `resolve_ep(encoder_path)` | fn | recognizer/ep_probe.rs:45 | D | — | `Result<EpChoice, String>`. Same hide-behind-trait note. |
| `resolve()` (`ort_lib`) | fn | recognizer/ort_lib.rs:33 | P | — | `Result<PathBuf, String>`. Module-level doc covers it. |
| `MelExtractor` + ctor + `extract` | struct/fn | recognizer/mel.rs:44/55/65 | D/N/D | n | OK to keep public; benches consume it. **Add `#[non_exhaustive]`**. |
| `SAMPLE_RATE`, `N_FFT`, `WIN_LENGTH`, `HOP_LENGTH`, `N_MELS`, `PREEMPH`, `LOG_ZERO_GUARD`, `NORM_EPS` | const | recognizer/mel.rs:28-40 | D | — | model-baked constants; documented. |

**Gap.** No `RecognizerInfo` / diagnostics accessor today. Tauri reads
`OrtParakeet::selected_ep()` directly; Mac has nothing equivalent.
Task 7 (`cli recognizer-info`) needs:

```rust
// land in core::diagnostics during Task 2 Commit D
pub struct RecognizerInfo {
    pub engine: &'static str,    // "FluidAudio" | "ort+sherpa-onnx"
    pub model_path: PathBuf,
    pub model_version: &'static str,
    pub ep: String,              // "ANE" | "DirectML" | "CPU" | …
}
pub fn recognizer_info() -> RecognizerInfo;
```

---

## stats (`core::stats`)

| pub item | kind | line | doc | NE? | notes |
|---|---|---|---|---|---|
| `StatsSummary { words_today, words_week, words_all_time, seconds_total }` | struct | stats/mod.rs:34 | D | n | `Serialize` derive, snake_case. **Add `#[non_exhaustive]`** — `chars_today`, `wpm_p50`, `total_dictations` etc. are likely additions. |
| `StatsSummary::empty()` | fn | stats/mod.rs:44 | N | — | |
| `set_store(store)` | fn | stats/mod.rs:58 | D | — | |
| `store()` | fn | stats/mod.rs:64 | D | — | `Option<&'static Arc<Store>>` — exposes `static`-borrow into the public API. Acceptable — singleton pattern is intentional. |
| `set_on_insert(cb)` | fn | stats/mod.rs:76 | D | — | excellent rationale: keeps core unaware of Tauri events. |
| `record_dictation(store, started_at_ms, duration_ms, wall_clock_ms, text)` | fn | stats/mod.rs:91 | D | — | five-arg fn — once the schema grows (app_bundle_id, language) consider a `Builder` or `RecordedDictation` struct. |
| `get_summary(store, now_ms)` | fn | stats/mod.rs:147 | D | — | `Result<StatsSummary, StoreError>`. `now_ms` injected for tests — good. |
| `reset(store)` | fn | stats/mod.rs:182 | D | — | |

---

## store (`core::store`)

| pub item | kind | line | doc | NE? | notes |
|---|---|---|---|---|---|
| `StoreError` | enum | store/mod.rs:21 | N | n | three variants (`Io`, `Sqlite`, `Migration`) with payload. Has `Display`, `Error::source`, `From<...>`. **Add `#[non_exhaustive]`** so a future `Locked` / `SchemaMismatch` variant doesn't break match arms in the shell. |
| `Store` | struct | store/mod.rs:65 | P | n | one private field. |
| `Store::open_or_init(path)` | fn | store/mod.rs:73 | D | — | |
| `Store::with_conn<R>(closure)` | fn | store/mod.rs:88 | D | — | |
| `apply_pending(conn)` (`migrations`) | fn | store/migrations.rs:40 | N | — | The `migrations` module itself is `mod migrations;` (private to `store`). Not part of public API. |

---

## verbose (`core::verbose`)

| pub item | kind | line | doc | NE? | notes |
|---|---|---|---|---|---|
| `enabled()` | fn | verbose.rs:24 | D | — | |
| `verbose_log!` (macro_rules) | macro | verbose.rs:32 | D | — | `#[macro_export]`. |

---

## ffi-c (`core::ffi_c`, `mod` is private but symbols ship)

These are `pub extern "C"` no-mangle symbols built into the staticlib for
the Windows C# shell to P/Invoke. Not part of the Rust-consumer public
API — but they ARE part of the core crate's external surface and a
breaking change to any of them ships as a Windows shell rebuild.

| pub item | kind | line | doc | NE? | notes |
|---|---|---|---|---|---|
| `OwDictationSnapshot` | struct | ffi_c.rs:34 | D | — | `#[repr(C)]`. 6 fields. |
| `ow_core_version()` | extern fn | ffi_c.rs:46 | D | — | |
| `ow_process_transcript(input, out_buf, out_cap)` | extern fn | ffi_c.rs:61 | D | — | unsafe; full safety contract documented. |
| `ow_dictation_snapshot(out)` | extern fn | ffi_c.rs:82 | D | — | |
| `ow_dictation_status_message(out_buf, out_cap)` | extern fn | ffi_c.rs:105 | D | — | |
| `ow_dictation_transcript(out_buf, out_cap)` | extern fn | ffi_c.rs:118 | D | — | |
| `ow_dictation_error_message(out_buf, out_cap)` | extern fn | ffi_c.rs:131 | D | — | |
| `ow_dictation_request_toggle()` | extern fn | ffi_c.rs:141 | D | — | |
| `ow_dictation_request_cancel()` | extern fn | ffi_c.rs:147 | D | — | |
| `ow_dictation_mark_loading_model()` | extern fn | ffi_c.rs:152 | N | — | |
| `ow_dictation_mark_capture_started()` | extern fn | ffi_c.rs:157 | N | — | |
| `ow_dictation_mark_capture_stopped(sample_count)` | extern fn | ffi_c.rs:162 | N | — | |
| `ow_dictation_deliver_transcript(text, confidence)` | extern fn | ffi_c.rs:171 | D | — | |
| `ow_dictation_deliver_error(message)` | extern fn | ffi_c.rs:186 | D | — | |
| `ow_audio_start_capture(err_buf, err_cap)` | extern fn | ffi_c.rs:202 | D | — | |
| `ow_audio_stop_capture()` | extern fn | ffi_c.rs:216 | N | — | |
| `ow_audio_is_capturing()` | extern fn | ffi_c.rs:221 | N | — | |
| `ow_audio_current_level()` | extern fn | ffi_c.rs:226 | N | — | |
| `ow_audio_drain_samples(out_buf, out_cap)` | extern fn | ffi_c.rs:239 | D | — | |

**Note.** `OwDictationSnapshot` is missing `download_bytes_done` /
`download_bytes_total` (added to `DictationSnapshot` later for the
download progress UI). The Windows C# shell can't currently surface the
download bar — flag for a follow-up after Task 2 Commit C, but not a
TASK-81 blocker.

---

## Gaps to fill in Task 2 / Task 3

### Missing modules

1. **`core::settings`.** Schema, atomic-flag handling, and persistence
   currently live in `apps/tauri/src-tauri/src/settings/mod.rs` (663
   lines). All of it is platform-agnostic JSON IO + serde + behavior
   caches that hot-path readers (fullscreen detector, dictation phase
   observer, MediaController) hit. **Move in Task 2 Commit B.**

2. **`core::diagnostics`.** Empty today. Lands in Task 2 Commit D with
   `RecognizerInfo`, `DiagnosticsReadout`, and the `CrashDumpReader`
   trait + `CrashDump` placeholder struct (concrete impl ships in
   TASK-78). Unblocks Task 7 (`cli recognizer-info`) and Task 8
   (`cli crash-dump` stub).

3. **`core::media_gate`.** The pause/resume gate logic + `PAUSED_BY_US`
   idempotency invariant currently live in `apps/tauri/src-tauri/src/lib.rs`
   (lines 50, 66-87, 94-106). Trait surface (`MediaController`,
   `PauseDiagnostic`) currently in
   `apps/tauri/src-tauri/src/media_control/mod.rs` is partially core-
   shaped already. **Move in Task 2 Commit A.**

### Missing prelude (Task 3)

```rust
// core/src/prelude.rs (to land)
pub use crate::audio::{
    AudioDeviceInfo, AudioEngine, SelectedDeviceStatus,
};
pub use crate::dictation::{
    DictationSnapshot, Injector, Phase, ToggleAction,
};
pub use crate::recognizer::{Recognizer, TranscribeResult};
pub use crate::transcript::{FillerLang};
pub use crate::stats::{StatsSummary};
pub use crate::store::{Store, StoreError};
// post Task 2 Commit D:
pub use crate::diagnostics::{
    CrashDump, CrashDumpReader, CrashId, DiagnosticsReadout,
    RecognizerInfo, default_crash_reader,
};
// post Task 2 Commit B:
pub use crate::settings::{Settings, SettingsStore};
// post Task 2 Commit A:
pub use crate::media_gate::{MediaController, PauseDiagnostic};
```

### `#[non_exhaustive]` checklist

Per Task 3, add to:

- `audio::AudioDeviceInfo` (audio.rs:47)
- `audio::SelectedDeviceStatus` (audio.rs:773)
- `dictation::DictationSnapshot` (dictation.rs:95) + planned `Phase` /
  `ToggleAction` enums when extracted from the `pub const u32` constants
- `transcript::FillerLang` (transcript.rs:26)
- `recognizer::TranscribeResult` (recognizer/mod.rs:44)
- `recognizer::mel::MelExtractor` (recognizer/mel.rs:44)
- `recognizer::download::ModelPaths` (recognizer/download.rs:21)
- `stats::StatsSummary` (stats/mod.rs:34)
- `store::StoreError` (store/mod.rs:21)
- (post-extraction) every new struct / enum landing in `core::settings`,
  `core::diagnostics`, `core::media_gate`.

Excepted, with explicit comment:

- `core::ffi_c::OwDictationSnapshot` (ffi_c.rs:34) — `#[repr(C)]` FFI
  surface; `#[non_exhaustive]` is incompatible with field-by-field
  layout guarantees the C# P/Invoke side relies on. Add a comment citing
  this so the Task 3 review doesn't bounce it.
- The `pub const u32` PHASE_* / TOGGLE_* — once converted to enums, the
  enums get `#[non_exhaustive]`; the consts stay as derived `pub const
  u32` aliases for the swift-bridge / C# FFI surface.

---

## Doc-comment punch list (for Task 3)

These are the items that need authoring (not summarizing) before
`#![warn(missing_docs)]` can land on `core/src/lib.rs`:

- `audio::AudioEngine` + every method on it (audio.rs:85, 112-149).
- `audio::audio_stop_capture`, `audio_drain_samples`, `audio_is_capturing`,
  `audio_get_selected_device_id`, `audio_preview_stop` (audio.rs:414, 422,
  591, 605, 758).
- `dictation::DictationSnapshot::*` accessors (dictation.rs:110-141).
- `dictation::dictation_snapshot`, `dictation_request_toggle`,
  `dictation_request_cancel`, `dictation_mark_loading_model`,
  `dictation_mark_capture_started`, `dictation_mark_capture_stopped`,
  `dictation_deliver_transcript`, `dictation_deliver_error`
  (dictation.rs:168, 187, 207, 228, 297, 343, 359, 424).
- `transcript::FillerLang`, `detect_lang`, `process` (transcript.rs:26,
  88, 97).
- `recognizer/fluidaudio::FluidAudioBridge::{new, default}`
  (recognizer/fluidaudio.rs:33, 38).
- `recognizer/ort_parakeet::OrtParakeet::{new}` (recognizer/ort_parakeet.rs:74).
- `recognizer/download::ModelPaths` (recognizer/download.rs:21).
- `core::core_version` (lib.rs:48).
- `stats::StatsSummary::empty` (stats/mod.rs:44).
- `store::StoreError` variant docs (store/mod.rs:21-25).
- `store::Store` struct doc (store/mod.rs:65).
- `ffi_c::ow_dictation_mark_*`, `ow_audio_stop_capture`,
  `ow_audio_is_capturing`, `ow_audio_current_level` (ffi_c.rs:152, 157,
  162, 216, 221, 226).
