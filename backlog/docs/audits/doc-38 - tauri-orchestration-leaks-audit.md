---
id: doc-38
title: Tauri shell orchestration leaks audit (2026-05-06)
type: spec
created_date: '2026-05-06'
---

# Tauri shell orchestration leaks audit

**Backlog parent:** TASK-81.1 (Plan Task 1)
**Spec:** `backlog/docs/specs/doc-24 - Library-API-audit-and-headless-CLI-—-design.md`
**Plan:** `backlog/docs/plans/doc-25 - Library-API-audit-and-headless-CLI-—-implementation-plan.md`

Classification of every symbol in `apps/tauri/src-tauri/src/` that mutates
global state, owns a state machine, or makes platform API calls. Per the
`openwhisper-orchestration-in-rust` rule, only platform glue stays in the
shell — everything else moves back to `core/`.

Codes:

- **(P)** Pure platform glue. Stays in shell.
- **(O)** Orchestration. Move to `core/`.
- **(M)** Mixed. Split — orchestration to core, platform glue stays.

Inspected files (deep): `lib.rs` (1295 LOC), `behavior.rs` (170 LOC),
`settings/mod.rs` (663 LOC), `focus.rs` (83 LOC),
`media_control/mod.rs` (95 LOC), `permissions/version_reset.rs` (134 LOC).

Skim-only (platform glue confirmed, item-level classification omitted):
`hotkey/{mac,windows,mod}.rs`, `fullscreen/{mac,windows,mod}.rs`,
`injection/{mac,windows,mod}.rs`, `media_control/{mac,windows}.rs`,
`permissions/{mac,mod}.rs`, `tray/mod.rs`. Spot-checked for orchestration
that should not be in those files; **none found** — they are correctly
platform-only.

---

## `lib.rs` — top-level shell

### Statics

| symbol | line | class | extraction target |
|---|---|---|---|
| `MEDIA_CONTROLLER: OnceLock<Arc<PlatformMediaController>>` | 36 | (P) | stays — holds the platform impl |
| `APP_HANDLE: OnceLock<tauri::AppHandle>` | 44 | (P) | Tauri-specific |
| `PAUSED_BY_US: AtomicBool` | 50 | **(O)** | `core::media_gate` (idempotency invariant — paired with the gate fns) |
| `LAST_PILL_POSITION: Mutex<Option<(f64, f64)>>` | 738 | (P) | pill positioning dedupe; HUD geometry is shell |
| `CACHED_AUDIO_DEVICE_STATE: Mutex<Option<AudioDeviceState>>` | 352 | (M) | the React-shaped struct lives in shell, but the underlying enumerate is core; cache pattern stays shell |

### Functions / commands

| symbol | line | kind | class | extraction target / note |
|---|---|---|---|---|
| `pause_audio_for_recording()` | 66-87 | fn | **(O)** | **`core::media_gate::pause(&impl MediaController, &MediaGateState)`**. Reads behavior cache, swaps idempotency flag, calls trait method, emits diagnostic event — last step is shell-side (Tauri emit); all others core. |
| `resume_audio_after_recording()` | 94-106 | fn | **(M)** | Idempotency check + swap → `core::media_gate::resume(...)`. Worker-thread spawn for the actual `resume_now` call stays shell (uses `std::thread::Builder` with a name; core can take a `Box<dyn FnOnce>` or just expose a sync `resume()` and let the shell own the spawn). |
| `product_name(app)` | 134 | fn | (P) | reads `tauri::Config`. |
| `phase_to_status(phase)` | 158-164 | fn | **(O)** | **`core::dictation::Phase::status_label(self) -> &'static str`**. Status string emission is *exactly* the case the orchestration-in-rust rule names. Already 3 strings ("recording", "transcribing", "idle") that the dictation-tick consumer reads. |
| `core_version` cmd | 166-169 | cmd | (P) | one-liner over `core::core_version()`. |
| `stats_get_summary` cmd | 173-180 | cmd | (M) | Resolves `now_ms` from `SystemTime` then calls `stats::get_summary`. **Move the `now_ms` resolution into a `stats::get_summary_now(store)` core helper** so the CLI doesn't repeat it. |
| `stats_reset` cmd | 186-189 | cmd | (P) | one-liner. |
| `do_toggle()` | 194-245 | fn | **(O)** | Five-step orchestration: mic-auth gate, request_toggle, mark_loading_model + spawn ensure_loaded thread, pause-gate, audio_start_capture, mark_capture_started; stop branch: stop_capture, mark_transcribing, spawn stop pipeline, resume-gate. **`core::dictation::run_toggle(&impl ToggleEnv)`** where `ToggleEnv` provides: `mic_authorized() -> bool`, `media_gate -> &impl MediaController`, and a `spawn(impl FnOnce + Send)` shim. The thread spawns themselves stay shell — the orchestration becomes a small state-machine driver in core. |
| `do_cancel()` | 248-254 | fn | **(M)** | stop_capture + drain + request_cancel + resume-gate. → `core::dictation::run_cancel(&impl ToggleEnv)`. Same env trait. |
| `dictation_toggle` cmd | 256-259 | cmd | (P) | one-liner after `do_toggle` extracts. |
| `media_get_last_pause_diagnostic` cmd | 266-269 | cmd | (P) | one-liner over `core::media_gate::last_pause_diagnostic()` post-extraction. |
| `open_automation_settings` cmd | 276-286 | cmd | (P) | shells `open x-apple.systempreferences:`. Fully platform glue. |
| `open_microphone_settings` cmd | 294-304 | cmd | (P) | same. |
| `open_pref_pane(target)` | 306-313 | fn | (P) | helper for the two cmds above. |
| `dictation_cancel` cmd | 315-318 | cmd | (P) | one-liner after `do_cancel` extracts. |
| `compute_audio_device_state()` | 359-391 | fn | **(M)** | mic-auth gate is shell. The mapping `audio_list_input_devices() → AudioDeviceState { devices, selected_id, selected_present, default_label }` is pure data shaping over core types. **Add `core::audio::audio_device_state() -> AudioDeviceState`**, lift the struct to core (Serialize-derived stays compatible with the React side via Tauri's serde). The `permissions::is_mic_authorized()` short-circuit becomes a `core` parameter or stays in the shell wrapper. |
| `refresh_audio_device_state_cache()` | 395-401 | fn | (M) | cache write; cache pattern stays shell (Tauri-specific event emit on change), inner compute moves with `compute_audio_device_state`. |
| `audio_get_device_state` cmd | 403-411 | cmd | (M) | reads cache, falls through to compute. Stays as a thin shell command after compute moves. |
| `audio_preview_start` cmd | 413-430 | cmd | **(O)** | is_recording gate + media-gate pause + `audio::audio_preview_start()`. Same orchestration shape as `do_toggle`'s begin. **`core::dictation::run_preview_start(&impl ToggleEnv)`**. |
| `audio_preview_stop` cmd | 432-436 | cmd | **(O)** | preview_stop + resume-gate. **`core::dictation::run_preview_stop(&impl ToggleEnv)`**. |
| `spawn_stop_pipeline()` | 445-496 | fn | **(O)** | drain → estimate_voiced_ms → set_voiced_ms → mark_capture_stopped → ensure_loaded → recognizer_transcribe → process(transcript) → deliver_transcript / deliver_error. **The whole pipeline is core territory: `core::dictation::run_stop_pipeline()` with the `verbose_log!` calls inline.** Shell keeps only the thread spawn. |
| `spawn_recognizer_warmup()` | 513-527 | fn | **(M)** | mark_loading_model → ensure_loaded → mark_loaded / deliver_error. **`core::dictation::run_warmup()`** — orchestration in core, thread spawn in shell. |
| `DEVICE_STATE_POLL_MS = 2000` | 532 | const | (M) | poll cadence. Shell config — but moves with the watcher to core if we go that way. Defer past TASK-81. |
| `hash_device_state(state)` | 534-538 | fn | (P) | `DefaultHasher` for change detection on the emitter loop. Shell utility. |
| `spawn_dictation_emitter(app)` | 540-576 | fn | **(M)** | Builds `DictationTick` payload from `dictation::dictation_snapshot()` + `audio::audio_current_level()` + `live_samples` derivation; emits `dictation_tick` event. **The "live_samples while recording" derivation (line 550-554) belongs in `core::dictation::Snapshot::live_samples(sample_rate_hz)`** (currently inline in the shell). Tauri event emit stays shell. |
| `spawn_audio_device_poller(app)` | 583-605 | fn | (P) | Tauri event emit + cpal poll thread. The polling pattern (re-enumerate, hash, conditionally emit) is shell. |
| `center_main_on_pill_monitor(app)` | 615-648 | fn | (P) | window placement math; uses `tauri::WebviewWindow` + `tauri::Monitor`. Fully shell. |
| `set_window_at_logical(window, x, y, scale)` | 661-682 | fn | (P) | platform DPI conversion for `set_position`. Shell. |
| `show_main_window` cmd | 688-700 | cmd | (P) | window manipulation. |
| `open_settings_window` cmd | 705-720 | cmd | (P) | window manipulation + `ow_navigate` event. |
| `set_pill_click_through` cmd | 722-732 | cmd | (P) | NSPanel passthrough. |
| `place_pill(app, monitor_origin)` | 764-831 | fn | (P) | HUD positioning + work-area math. Shell. |
| `apply_fullscreen_state(app, is_fullscreen)` | 843-876 | fn | **(M)** | Logic: `suppress = is_fullscreen && !show_in_fullscreen`; `was_recording = suppress && is_recording`. **The gating decision (`suppress` + `was_recording`) belongs in `core::dictation::fullscreen_action(is_fullscreen, show_in_fullscreen, is_recording) -> FullscreenAction { hide_pill, detach_hotkey, cancel_recording }`**. Pill hide / hotkey detach / `do_cancel` calls themselves stay shell (each is a platform op). |
| `reposition_pill` cmd | 878-889 | cmd | (P) | dispatches to `place_pill` on the main thread. |
| `run()` setup body | 947-1255 | fn | (M) | Mostly platform glue (NSPanel conversion, plugin registration, hotkey install, fullscreen detector, AX watcher, tray install, pill refresh thread). **Orchestration leaks inside `setup`:** boot-time hydration sequence calls `settings::load_*` (5 sites) — those load fns are themselves orchestration (see `settings/mod.rs` below). Once `core::settings` lands, these call sites become `core::settings::load_all(path)` once. |
| `tauri::generate_handler![...]` | 1257-1292 | macro | (P) | wires commands to invoke handler. Shell. |

---

## `behavior.rs` — behavior caches + commands

| symbol | line | kind | class | extraction target |
|---|---|---|---|---|
| `SHOW_IN_FULLSCREEN: AtomicBool` | 20 | static | **(O)** | **`core::settings::cache::SHOW_IN_FULLSCREEN`** (or part of a `BehaviorCache` struct on `core::settings`). Lock-free hot-path mirror of a setting — the lock-free pattern is fine, but the *setting* is a core concept. |
| `PAUSE_AUDIO: AtomicBool` | 21 | static | **(O)** | `core::settings::cache::PAUSE_AUDIO_DURING_DICTATION`. |
| `BT_RESUME_DELAY_MS: AtomicU64` | 27 | static | **(O)** | `core::settings::cache::BT_RESUME_DELAY_MS`. Read by the platform MediaController on every resume. |
| `show_in_fullscreen()` / `set_show_in_fullscreen_cache()` | 29-35 | fn | (O) | move with the static. |
| `pause_audio_during_dictation()` / `set_pause_audio_cache()` | 37-43 | fn | (O) | move with the static. |
| `bt_resume_delay_ms()` / `set_bt_resume_delay_ms_cache()` | 45-51 | fn | (O) | move with the static. |
| `current_or_default()` | 53-55 | fn | (O) | helper used only by the `behavior_set_*` cmds; moves with the schema. |
| `apply_collection_behavior(app, show)` | 73-95 | fn | **(P)** | NSPanel `set_collection_behavior` (Mac) / `set_visible_on_all_workspaces` (Win). Stays. |
| `behavior_get_show_in_fullscreen` cmd | 97-100 | cmd | (P) | one-liner after extraction. |
| `behavior_set_show_in_fullscreen(app, enabled)` cmd | 102-114 | cmd | **(M)** | save + cache update + emit. The save+cache pattern moves to `core::settings::set_show_in_fullscreen(store, value)`; the Tauri emit + the call into `apply_collection_behavior` stay shell. |
| `behavior_get_pause_audio_during_dictation` cmd | 116-119 | cmd | (P) | one-liner. |
| `behavior_set_pause_audio_during_dictation(app, enabled)` cmd | 121-133 | cmd | (M) | same shape as the show_in_fullscreen setter. |
| `behavior_get_bt_resume_delay_ms` cmd | 135-138 | cmd | (P) | one-liner. |
| `behavior_set_bt_resume_delay_ms(app, delay_ms)` cmd | 140-162 | cmd | **(M)** | The clamp `min(10_000)` is a core invariant that should live next to the schema. Save + cache + emit pattern same as above. |
| `schema_default_bt_resume_delay_ms()` | 168-170 | fn | (O) | wrapper over `default_bt_resume_delay_ms` from `settings/mod.rs`. Lives with the schema in core. |

---

## `settings/mod.rs` — the big one (663 LOC)

Every type and function in this file is **(O)** orchestration — JSON IO,
serde schema, atomic-flag handling, cache statics, and the Tauri commands
that wrap them. The Tauri commands themselves are (M): the `#[tauri::command]`
shell stays, the body becomes a one-liner over `core::settings::*`.

### Schema types — all (O), move to `core::settings`

| symbol | line |
|---|---|
| `enum HotkeyKind { ModifierTap, Chord }` | 21 |
| `struct HotkeyConfig { kind, code, mods }` | 30 |
| `impl HotkeyConfig::{modifier_tap, chord}` | 37-58 |
| `struct HotkeySettings { toggle, cancel }` | 61 |
| `enum HotkeyTarget { Toggle, Cancel }` + `as_str` | 70-82 |
| `default_toggle_hotkey()` / `default_cancel_hotkey()` / `default_settings()` | 84-104 |
| `struct SettingsFile` | 112 (private — internal envelope) |
| `struct StatsSettings { user_wpm }` + clamp | 133-164 |
| `struct PillSettings { follow_active_screen }` | 167-177 |
| `struct AudioSettings { device_id }` | 179-189 |
| `struct BehaviorSettings { show_in_fullscreen, pause_audio_during_dictation, bt_resume_delay_ms }` | 191-240 |
| `USER_WPM_MIN`, `USER_WPM_MAX`, `default_user_wpm`, `default_pause_audio_during_dictation`, `default_bt_resume_delay_ms` | 143-230 |

### Statics — all (O)

| symbol | line |
|---|---|
| `CURRENT: Mutex<Option<HotkeySettings>>` | 242 |
| `AUDIO_CURRENT: Mutex<Option<AudioSettings>>` | 243 |
| `BEHAVIOR_CURRENT: Mutex<Option<BehaviorSettings>>` | 244 |
| `STATS_CURRENT: Mutex<Option<StatsSettings>>` | 245 |
| `FOLLOW_ACTIVE_SCREEN: AtomicBool` | 251 |

### Load / save fns — all (O), with one (M) seam

| symbol | line | note |
|---|---|---|
| `settings_path(app)` | 261-267 | (M) — Tauri's `app_config_dir()` is shell; once core takes a `&Path`, the resolution stays shell. |
| `read_file(app)` | 269-277 | (M) — same pattern: parameterize over a path. |
| `write_file(app, file)` | 279-287 | (M) — same. |
| `merge_loaded(file)` | 289-297 | (O) |
| `load_settings(app)` | 304-316 | (O) |
| `current_settings()` | 318-320 | (O) |
| `save_settings(app, settings)` | 322-339 | (O) |
| `current_stats_settings()` | 341-343 | (O) |
| `load_stats_settings(app)` | 347-357 | (O) |
| `save_stats_settings(app, settings)` | 359-379 | (O) |
| `current_audio_settings()` | 381-383 | (O) |
| `load_audio_settings(app)` | 389-399 | (O) |
| `save_audio_settings(app, settings)` | 401-423 | (O) |
| `current_behavior_settings()` | 425-427 | (O) |
| `load_behavior_settings(app)` | 433-443 | (O) |
| `save_behavior_settings(app, settings)` | 445-470 | (O) |
| `update_slot(app, target, config)` | 472-483 | (O) |
| `save_pill_settings(app, settings)` | 537-556 | (O) |
| `current_pill_settings()` | 257-259 | (O) |
| `follow_active_screen()` | 253-255 | (O) |

### Tauri commands — all (M), become 1-liner wrappers

`settings_get_hotkeys` (485), `settings_set_hotkey` (490),
`settings_reset_hotkey` (501), `settings_capture_hotkey_start` (515),
`settings_capture_hotkey_cancel` (520), `audio_set_device` (525),
`settings_get_pill` (558), `settings_set_pill_follow` (563),
`settings_get_stats` (568), `settings_set_user_wpm` (579).

After Task 2 Commit B these all collapse into:

```rust
#[tauri::command]
fn settings_get_hotkeys(state: State<'_, Arc<Settings>>) -> HotkeySettings {
    state.hotkeys()
}
```

`audio_set_device` is special — its body **also** calls
`openwhisper_core::audio::audio_set_selected_device_id(normalized)`. After
extraction, that propagation lives inside `Settings::set_audio_device(...)` in
core, not at the call site.

The hotkey-capture commands (`settings_capture_hotkey_*`) cross into
`crate::hotkey::*`, which is the platform-glue path (CGEventTap on Mac,
WH_KEYBOARD_LL on Win). The `set_capture_active(active, target)` call
itself is hotkey-backend state — stays shell. Only the persistence side
moves to core.

---

## `focus.rs`

| symbol | line | class | note |
|---|---|---|---|
| `extern "C" fn AXIsProcessTrusted()` | 22 | (P) | ApplicationServices framework; Mac-only. |
| `bring_main_to_front(app)` | 30-37 | (P) | Tauri main-window manipulation. |
| `install_ax_watcher(app)` | 41-53 | (P) | spawns the watcher thread. |
| `ax_watch_loop(app)` | 56-83 | (P) | polls AX trust + brings window forward on edge. The "bring window forward" is shell; the policy "edge-only fire on false→true" is small enough to leave alongside the platform poll loop. |

Whole file is platform glue. Stays.

---

## `media_control/mod.rs`

| symbol | line | class | extraction target |
|---|---|---|---|
| `pub trait MediaController` | 8-14 | **(O)** | **Move trait surface to `core::media_gate::MediaController`**. Platform impls (`mac.rs`, `windows.rs`) stay in shell and `impl core::media_gate::MediaController for MacMediaController`. |
| `pub struct PauseDiagnostic { reason, detail }` | 28-35 | **(O)** | **`core::media_gate::PauseDiagnostic`**. The `reason` field is a `&'static str` tag — fine for v1, but flag for `#[non_exhaustive]` + a future enum once the variants stabilize. |
| `last_pause_diagnostic()` (Mac) | 38-40 | (M) | core wrapper dispatches to a registered controller's diagnostic; Mac side fills it in. |
| `last_pause_diagnostic()` (non-Mac) | 43-45 | (M) | returns `None`; same shape after extraction. |
| `probe_authorization()` (Mac) | 52-54 | (M) | Mac-only side-effect-free probe; trait method on the controller. |
| `probe_authorization()` (non-Mac) | 57-59 | (M) | returns `None`. |
| `to_ui(d)` (Mac) | 62-71 | (P) | converts the platform-side `mac::PauseDiagnostic` to the cross-platform shape. Stays shell-side as a `From` impl in `media_control/mac.rs`. |
| `pub use mac::MacMediaController as PlatformMediaController` | 19 | (P) | re-export. |
| `pub use windows::WindowsMediaController as PlatformMediaController` | 76 | (P) | re-export. |
| Linux fallback `PlatformMediaController` (no-op) | 79-94 | (P) | no-op stays in shell so Linux builds. |

---

## `permissions/version_reset.rs`

| symbol | line | class | note |
|---|---|---|---|
| `MARKER_FILE`, `TCC_SERVICES` | 54-57 | (P) | Mac-only constants. |
| `current_cdhash()` | 60-81 | (P) | shells `codesign -d --verbose=4`. |
| `reset_if_version_changed(app)` (Mac) | 84-131 | (P) | reads `app_config_dir`, runs `tccutil reset`. Tauri-shaped via `app: &AppHandle`. The marker-file IO is platform-agnostic but the whole feature is Mac TCC, so leaving it in the shell is correct. |
| `reset_if_version_changed(app)` (non-Mac) | 134 | (P) | no-op. |

Whole file is platform glue. Stays.

---

## Other files (skim-only confirmation)

- `hotkey/{mac,windows,mod}.rs` — CGEventTap (mac) / WH_KEYBOARD_LL (win)
  + the `hotkey_retry` / `hotkey_status_current` Tauri commands that are
  thin wrappers over those backends. **(P)** — stays.
- `fullscreen/{mac,windows,mod}.rs` — Mac `_NSWindowDidEnterFullScreen`
  notification observer + work-area math; Win `WH_SHELL` hook +
  `IShellWindows` enumeration. **(P)** — stays. The `is_active()` /
  `last_pill_monitor()` / `cursor_monitor()` / `find_tauri_monitor()`
  helpers exposed to `lib.rs` are platform queries, not orchestration.
- `injection/{mac,windows,mod}.rs` — clipboard + Cmd+V / Ctrl+V synthesis
  via CGEventPost / SendInput, behind a `TauriInjector` that registers
  itself via `core::dictation::set_injector`. **(P)** — stays.
- `media_control/{mac,windows}.rs` — AppleScript MediaRemote bindings
  (mac.rs:547 LOC) + SMTC MediaTransportControlsService (windows.rs:307
  LOC). **(P)** — stays.
- `permissions/{mac,mod}.rs` — AVCaptureDevice authorization + mic banner
  state. **(P)** — stays.
- `tray/mod.rs` — Tauri tray menu construction with phase-aware labels.
  The phase-aware label mapping (`Phase → "Stop dictation" / "Start
  dictation"`) is borderline (O), but it's a string the tray menu
  consumes locally — leaving it next to the menu wiring is pragmatic.
  **(P)** — stays.

---

## Extraction checklist for Task 2

Numbered to match the canonical commit shape in
`backlog/docs/plans/doc-25 - Library-API-audit-and-headless-CLI-—-implementation-plan.md`.

### Commit A — `core::media_gate`

- [ ] New module `core/src/media_gate.rs`. Re-export from
      `core/src/lib.rs`.
- [ ] Move `MediaController` trait from
      `apps/tauri/src-tauri/src/media_control/mod.rs:8-14` to
      `core::media_gate::MediaController`.
- [ ] Move `PauseDiagnostic` struct from
      `apps/tauri/src-tauri/src/media_control/mod.rs:28-35` to
      `core::media_gate::PauseDiagnostic`. Add `#[non_exhaustive]`.
- [ ] Move `PAUSED_BY_US: AtomicBool` from `lib.rs:50` into a
      `MediaGateState` struct in `core::media_gate`. Expose as a static
      via `core::media_gate::default_gate_state() -> &'static MediaGateState`.
- [ ] Move pause/resume gate logic from `lib.rs:66-87` and `lib.rs:94-106`
      into:
      ```rust
      pub fn pause(controller: &impl MediaController, gate: &MediaGateState) -> Option<PauseDiagnostic>;
      pub fn resume(controller: &impl MediaController, gate: &MediaGateState);
      ```
      Last-diagnostic emission stays shell-side via a callback the shell
      registers (mirrors `stats::set_on_insert`).
- [ ] Update shell `pause_audio_for_recording` / `resume_audio_after_recording`
      to one-liners that call `core::media_gate::*`.
- [ ] Move `last_pause_diagnostic()` / `probe_authorization()` accessors
      to `core::media_gate`.

### Commit B — `core::settings`

- [ ] New module `core/src/settings/`. Sub-files mirror the schema
      blocks (`hotkey.rs`, `audio.rs`, `behavior.rs`, `pill.rs`, `stats.rs`,
      `file.rs`).
- [ ] Move every schema type from `apps/tauri/src-tauri/src/settings/mod.rs`
      (HotkeyKind, HotkeyConfig, HotkeySettings, HotkeyTarget,
      AudioSettings, BehaviorSettings, PillSettings, StatsSettings,
      SettingsFile envelope). Add `#[non_exhaustive]` to every public
      struct/enum that doesn't have an explicit FFI-stable reason not to.
- [ ] Move all default fns and constants
      (USER_WPM_MIN/MAX, default_user_wpm, default_pause_audio_during_dictation,
      default_bt_resume_delay_ms).
- [ ] Move all five caches (CURRENT, AUDIO_CURRENT, BEHAVIOR_CURRENT,
      STATS_CURRENT, FOLLOW_ACTIVE_SCREEN). Lock-free atomic mirrors
      stay atomic.
- [ ] Move `behavior.rs`'s `SHOW_IN_FULLSCREEN` / `PAUSE_AUDIO` /
      `BT_RESUME_DELAY_MS` atomics into the same cache module.
- [ ] Convert `read_file(app)` / `write_file(app, file)` / `settings_path(app)`
      to take a `&Path` parameter. Shell wrapper resolves
      `app.path().app_config_dir()` once and passes it in.
- [ ] Move every `load_*` / `save_*` / `current_*` / `update_slot`
      function. Shell calls become `core::settings::*`.
- [ ] Move the `bt_resume_delay_ms.min(10_000)` clamp from
      `behavior.rs:154` into `BehaviorSettings::set_bt_resume_delay_ms` (or
      a `Settings::set_bt_resume_delay_ms`).
- [ ] Tauri commands in `behavior.rs` and `settings/mod.rs` collapse to
      1-liner `#[tauri::command]` wrappers; persistent emission of
      `behavior_*_changed` / `settings_*_changed` events stays shell.

### Commit C — `core::dictation` augmentation

- [ ] Move `phase_to_status` (`lib.rs:158-164`) into
      `Phase::status_label(self) -> &'static str` (or a free fn
      `core::dictation::phase_status_label(phase: u32) -> &'static str` if
      we don't extract Phase to an enum yet — but Task 3 *will* extract
      it).
- [ ] Move the live-samples derivation from
      `spawn_dictation_emitter` (`lib.rs:550-554`) into
      `DictationSnapshot::live_samples(sample_rate_hz: u64) -> u64`.
- [ ] Move `do_toggle()` / `do_cancel()` / `audio_preview_start` /
      `audio_preview_stop` orchestration bodies (`lib.rs:194-245`,
      `248-254`, `413-430`, `432-436`) into `core::dictation::run_*` fns
      that take a `ToggleEnv` trait providing mic-auth, media-gate, and
      a `spawn` shim. Shell impls the trait.
- [ ] Move `spawn_stop_pipeline` body (`lib.rs:445-496`) into
      `core::dictation::run_stop_pipeline(env: &impl StopEnv)`. Shell
      keeps the thread spawn around it.
- [ ] Move `spawn_recognizer_warmup` body (`lib.rs:513-527`) into
      `core::dictation::run_warmup()`. Same pattern.
- [ ] Move `apply_fullscreen_state` gating decision (`lib.rs:843-876`)
      into `core::dictation::fullscreen_action(is_fullscreen,
      show_in_fullscreen, is_recording) -> FullscreenAction { hide_pill,
      detach_hotkey, cancel_recording }`. Shell consumes the return value
      and dispatches to platform ops.

### Commit D — `core::diagnostics`

- [ ] New module `core/src/diagnostics/`. Re-export from `core/src/lib.rs`.
- [ ] Define `RecognizerInfo { engine, model_path, model_version, ep }`.
      Populate from FluidAudioBridge (Mac) / OrtParakeet (Win — already
      has `selected_ep()`).
- [ ] Define `DiagnosticsReadout` that aggregates `RecognizerInfo` +
      crate version + paths.
- [ ] Define placeholder `pub struct CrashDump { /* TASK-78 fills */ }`
      `#[non_exhaustive]` and `pub trait CrashDumpReader { fn list() ->
      Vec<CrashId>; fn read(&self, id: &CrashId) -> Result<CrashDump,
      ReadError>; }`. Plus `pub fn default_crash_reader() ->
      Option<Box<dyn CrashDumpReader>>` returning `None` until TASK-78.

### Commit E — audio device-state shaping

- [ ] Move `AudioDeviceState` struct from `lib.rs:338-344` to
      `core::audio` (or `core::diagnostics`).
- [ ] Move `compute_audio_device_state` body (`lib.rs:359-391`) into
      `core::audio::audio_device_state(authorized: bool) -> AudioDeviceState`.
      Shell passes `permissions::is_mic_authorized()`. The cache pattern
      and Tauri event emission stay shell.

### Out of scope for Task 2 (defer)

- The `tray/mod.rs` phase-aware label mapping (very small, low value to
  extract — leave alongside menu wiring).
- The `DEVICE_STATE_POLL_MS` cadence + the device poller thread —
  expanding this into a `core::audio::DeviceWatcher` is a bigger design
  decision; defer to a v1.x follow-up.
- Tauri event names (`dictation_tick`, `audio_device_state`,
  `media_pause_diagnostic_changed`, `behavior_*_changed`,
  `settings_*_changed`, `stats_changed`, `ow_navigate`). These are
  Tauri-specific and stay shell.

---

## Verification (Task 1 ACs)

- AC #1 — Audit doc 1 (core public API) committed: `doc-37`.
- AC #2 — Audit doc 2 (this doc) committed with P/O/M classification +
  line-number citations.
- AC #3 — Concrete extraction checklist named in the section above
  (Commit A → E), keyed to source locations.
- AC #4 — Coverage:
  - Audit doc 1 enumerates every `pub` in `core/src/` (16 files) by
    capability with file:line for each.
  - Audit doc 2 covers every shell symbol in `lib.rs` (1295 LOC),
    `behavior.rs`, `settings/mod.rs`, `focus.rs`,
    `media_control/mod.rs`, `permissions/version_reset.rs` that mutates
    global state or makes platform API calls. Pure platform-glue files
    confirmed at the file level.
