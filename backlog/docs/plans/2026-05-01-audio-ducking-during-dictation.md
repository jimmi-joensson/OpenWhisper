# Pause audio during dictation — implementation plan

**Backlog parent:** TASK-61
**Spec:** `backlog/docs/specs/2026-05-01-audio-ducking-during-dictation.md`
**Date:** 2026-05-01

Each `### Task N:` heading maps 1:1 to a Backlog subtask `TASK-61.N`. Sequential — Task 1 lands the schema, cache, and commands the rest depend on; Task 2 lands the cross-platform trait + phase observer; Tasks 3 and 4 land the platform impls; Task 5 lands the React UI; Task 6 covers it with Playwright.

---

### Task 1: Settings schema + Rust commands + in-process cache

**Goal.** Add `pause_audio_during_dictation: bool` (default **true**) to `BehaviorSettings`, expose `behavior_get_pause_audio_during_dictation` / `behavior_set_pause_audio_during_dictation` Tauri commands, emit `behavior_pause_audio_changed` on writes, and keep an `AtomicBool` cache so the phase observer reads it without disk I/O.

**Files.** `apps/tauri/src-tauri/src/settings/mod.rs` (extend `BehaviorSettings` struct + `save_behavior_settings`), `apps/tauri/src-tauri/src/behavior.rs` (cache + commands), `apps/tauri/src-tauri/src/lib.rs` (handler registration + boot-time hydrate).

**Steps.**

1. Read the current `BehaviorSettings` struct in `settings/mod.rs` (existing shape includes `show_in_fullscreen`). Add:
   ```rust
   pub struct BehaviorSettings {
       #[serde(default = "default_show_in_fullscreen")]
       pub show_in_fullscreen: bool,
       #[serde(default = "default_pause_audio_during_dictation")]
       pub pause_audio_during_dictation: bool,
   }

   fn default_pause_audio_during_dictation() -> bool { true }
   ```
   Update `Default for BehaviorSettings` to set the new field to `true`.
2. Read `apps/tauri/src-tauri/src/behavior.rs`. Add next to `SHOW_IN_FULLSCREEN`:
   ```rust
   static PAUSE_AUDIO: AtomicBool = AtomicBool::new(true);

   pub fn pause_audio_during_dictation() -> bool {
       PAUSE_AUDIO.load(Ordering::Relaxed)
   }

   pub fn set_pause_audio_cache(value: bool) {
       PAUSE_AUDIO.store(value, Ordering::Relaxed);
   }

   #[tauri::command]
   pub fn behavior_get_pause_audio_during_dictation() -> bool {
       pause_audio_during_dictation()
   }

   #[tauri::command]
   pub fn behavior_set_pause_audio_during_dictation(
       app: AppHandle,
       enabled: bool,
   ) -> Result<(), String> {
       // 1. Mutate BehaviorSettings via save_behavior_settings(...)
       // 2. set_pause_audio_cache(enabled)
       // 3. app.emit("behavior_pause_audio_changed", enabled)
   }
   ```
   Mirror the exact signature of `behavior_set_show_in_fullscreen` for `save_behavior_settings` invocation — read the current struct, mutate the one field, save.
3. In `lib.rs::run()`, register the two new commands in `tauri::generate_handler![…]` alongside the existing behavior commands.
4. In `lib.rs::setup()`, after `let loaded_behavior = settings::load_behavior_settings(&app_handle)`, add `behavior::set_pause_audio_cache(loaded_behavior.pause_audio_during_dictation)` so the cache is hot before `spawn_dictation_emitter` starts.
5. `cargo check` clean from `apps/tauri/src-tauri/`.
6. Smoke: write a `cargo test` covering load → mutate → save → reload round-trip on `BehaviorSettings.pause_audio_during_dictation`.

**Outcome ACs (Backlog).**

- `BehaviorSettings` schema includes `pause_audio_during_dictation: bool`, default `true`, `serde(default)` round-trip safe.
- `behavior.rs` exposes `pause_audio_during_dictation()` cache reader, `set_pause_audio_cache(...)` writer, and the two Tauri commands.
- `behavior_set_pause_audio_during_dictation` persists, updates cache, emits `behavior_pause_audio_changed` with the new boolean.
- Commands registered in `generate_handler!`; cache hydrated in `setup()` from loaded settings.
- `cargo check` clean; round-trip test green.

---

### Task 2: MediaController trait + phase observer

**Goal.** New `media_control` module with a `MediaController` trait, a no-op default impl, and a phase observer that fires `pause_now()` / `resume_now()` on RECORDING entry/exit when the cache says enabled. Real platform impls are stubbed (return `false` from `pause_now`, no-op `resume_now`); Tasks 3 and 4 fill them in.

**Files.** `apps/tauri/src-tauri/src/media_control/mod.rs` (new), `apps/tauri/src-tauri/src/media_control/mac.rs` (new, stub), `apps/tauri/src-tauri/src/media_control/windows.rs` (new, stub), `apps/tauri/src-tauri/src/lib.rs` (wire observer into emitter loop).

**Steps.**

1. New `media_control/mod.rs`:
   ```rust
   pub trait MediaController: Send + Sync {
       fn pause_now(&self) -> bool;  // true if we paused something
       fn resume_now(&self);
   }

   #[cfg(target_os = "macos")]
   mod mac;
   #[cfg(target_os = "macos")]
   pub use mac::MacMediaController as PlatformMediaController;

   #[cfg(target_os = "windows")]
   mod windows;
   #[cfg(target_os = "windows")]
   pub use windows::WindowsMediaController as PlatformMediaController;

   #[cfg(not(any(target_os = "macos", target_os = "windows")))]
   pub struct PlatformMediaController;
   #[cfg(not(any(target_os = "macos", target_os = "windows")))]
   impl MediaController for PlatformMediaController {
       fn pause_now(&self) -> bool { false }
       fn resume_now(&self) {}
   }
   ```
2. Stub `mac.rs` and `windows.rs` each defining a struct that implements the trait with `false` / no-op bodies + a `new() -> Self` constructor. Real bodies land in Tasks 3 and 4.
3. In `lib.rs`, store `Arc<PlatformMediaController>` in a `OnceLock`, initialized in `setup()`. Inside `spawn_dictation_emitter`, track `let mut prev_phase: u32 = PHASE_IDLE;` and `let mut paused_by_us = false;`. After each tick:
   ```rust
   let current_phase = snap.phase();
   let cache_on = behavior::pause_audio_during_dictation();
   let entered_recording = prev_phase != PHASE_RECORDING && current_phase == PHASE_RECORDING;
   let exited_recording = prev_phase == PHASE_RECORDING && current_phase != PHASE_RECORDING;
   if entered_recording && cache_on {
       paused_by_us = MEDIA_CONTROLLER.get().map_or(false, |c| c.pause_now());
   }
   if exited_recording && paused_by_us {
       if let Some(c) = MEDIA_CONTROLLER.get() {
           c.resume_now();
       }
       paused_by_us = false;
   }
   prev_phase = current_phase;
   ```
4. Confirm the emitter tick rate (~20 Hz) is fast enough: the worst-case delay between PHASE_RECORDING entry and `pause_now()` firing is one tick (~50 ms). Acceptable for v1; if it becomes audible, drop the rate to 10 ms or hook the transition directly.
5. `cargo check` clean on Mac and Windows targets (use `cargo check --target x86_64-pc-windows-gnu` if cross-compile is set up; otherwise verify on the matching machine).
6. Manual smoke (with stub impls — should be invisible): start dictation, confirm no panic, no media changes, no log spam. The observer fires but the stub impls do nothing.

**Outcome ACs (Backlog).**

- `media_control` module exists with `MediaController` trait + `PlatformMediaController` cfg-gated alias.
- Stub Mac + Windows impls compile with `pause_now() -> false`, `resume_now()` no-op.
- `spawn_dictation_emitter` detects RECORDING entry/exit, calls `pause_now`/`resume_now` only when cache=on and only when entry-paused-something.
- Observer state (`prev_phase`, `paused_by_us`) is local to the emitter thread — no new shared state, no new threads.
- `cargo check` clean on the host platform; manual smoke shows no behavior change with stub impls.

---

### Task 3: macOS MediaController implementation

**Goal.** Real `MacMediaController` that uses MediaRemote (private framework) for pause/play and CoreAudio (default-output endpoint volume) for fade + mute fallback.

**Files.** `apps/tauri/src-tauri/src/media_control/mac.rs` (replace stub), `apps/tauri/src-tauri/Cargo.toml` (add `objc2`/`core-foundation` deps if not already pulled in by sibling code).

**Steps.**

1. `pause_now` flow:
   - Resolve MediaRemote symbols once (lazy `OnceLock<MediaRemoteFns>`):
     - `dlopen("/System/Library/PrivateFrameworks/MediaRemote.framework/MediaRemote", RTLD_LAZY)`.
     - `dlsym(handle, "MRMediaRemoteSendCommand")` (signature `(c_uint, *const c_void)`).
     - `dlsym(handle, "MRMediaRemoteGetNowPlayingApplicationIsPlaying")` (signature `(dispatch_queue_t, block)`).
     - If `dlopen` or any `dlsym` returns null, log a one-time warning and skip — fall through to `mute_fade_fallback`.
   - Read default output device volume via CoreAudio (`AudioObjectGetPropertyData` with `kAudioDevicePropertyVolumeScalar` on `kAudioHardwarePropertyDefaultOutputDevice`'s master channel). Save as `original_volume: f32`.
   - Ramp endpoint volume from `original_volume` to `0.0` over 200 ms (20 steps × 10 ms `thread::sleep`).
   - Send `MRMediaRemoteSendCommand(2, ptr::null())` (kMRPause = 2).
   - Restore endpoint volume to `original_volume` (immediate snap; paused source is silent so user hears nothing).
   - Re-poll `MRMediaRemoteGetNowPlayingApplicationIsPlaying` once with a ~100 ms timeout. If still `true`, the pause didn't take — keep endpoint at 0 (mute fallback) and store `is_muted = true` on `self`.
   - Store `original_volume`, `paused_via_media_remote` flag, and `is_muted` flag inside the controller's `Mutex<State>` so `resume_now` knows what to undo.
   - Return `true` if either pause-send or mute-fallback engaged; `false` if MediaRemote symbol resolution failed AND CoreAudio failed (highly unusual).
2. `resume_now` flow:
   - Lock `state`. If `paused_via_media_remote`:
     - Snap endpoint volume to 0.0.
     - Send `MRMediaRemoteSendCommand(0, ptr::null())` (kMRPlay = 0).
     - Ramp endpoint volume from 0.0 to `original_volume` over 200 ms.
   - Else if `is_muted` (mute fallback):
     - Ramp endpoint volume from current → `original_volume` over 200 ms.
   - Clear flags.
3. Serialize fade ops with the existing `state` mutex so rapid tap-tap-tap-toggle doesn't queue overlapping fades. Each call holds the mutex through its 200 ms ramp; the next call waits.
4. Add a small Rust-side trace on each entry: `tracing::debug!("MacMediaController::pause_now: paused_via_media_remote={paused_via_media_remote}, original_volume={original_volume}")`. Useful when the BT-mono regression ever recurs.
5. Manual smoke matrix on Mac:
   - **Spotify playing → record** → Spotify pauses, fade-out audible, no mono switch (because Spotify pauses fast enough that the BT link doesn't sit in HFP-with-music for long). Stop recording → Spotify resumes, fade-in audible.
   - **YouTube tab in Safari playing → record** → media-key pause hits Safari's now-playing tab; behavior matches Spotify.
   - **System Settings → Sound test sound → record** → no media-session app, mute fallback engages; record-end restores volume.
   - **Cancel mid-recording (Esc)** → music resumes (cancel path triggers PHASE_RECORDING exit just like stop).
   - **Setting=off** → no audio change on record-start or record-end. Setting=on → restore behavior.
6. `cargo check` clean.

**Outcome ACs (Backlog).**

- `MacMediaController::pause_now` resolves MediaRemote symbols via dlopen/dlsym, falls back to mute-only when symbols absent.
- Pause flow: ramp endpoint → 0, send kMRPause, snap endpoint to original.
- Resume flow: snap endpoint to 0, send kMRPlay, ramp endpoint → original.
- Mute fallback: ramp to 0, hold while recording, ramp back to original on resume.
- State (`original_volume`, flags) protected by a mutex; concurrent calls serialize.
- Manual smoke matrix passes for Spotify, Safari/YouTube, no-media-app, cancel-mid-recording, and setting=off.

---

### Task 4: Windows MediaController implementation

**Goal.** Real `WindowsMediaController` that uses SMTC for pause/play of every playing session and Core Audio endpoint volume for fade + mute fallback.

**Files.** `apps/tauri/src-tauri/src/media_control/windows.rs` (replace stub), `apps/tauri/src-tauri/Cargo.toml` (add `windows` crate features `Windows_Media_Control`, `Windows_Foundation`, `Win32_Media_Audio`, `Win32_System_Com`).

**Steps.**

1. `pause_now` flow:
   - WinRT init: `RoInitialize(RO_INIT_MULTITHREADED)` (idempotent — runtime tolerates re-init).
   - `GlobalSystemMediaTransportControlsSessionManager::RequestAsync()?.get()?` — synchronous wait OK on the emitter thread for v1; if it stalls we'll move to a worker.
   - `manager.GetSessions()?` returns `IVectorView<GlobalSystemMediaTransportControlsSession>`. Iterate and collect the ones with `GetPlaybackInfo()?.PlaybackStatus()? == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing` into `paused_sessions: Vec<GlobalSystemMediaTransportControlsSession>`.
   - Read default output device endpoint volume via `IMMDeviceEnumerator::GetDefaultAudioEndpoint(eRender, eMultimedia)` → `Activate(IID_IAudioEndpointVolume, ...)` → `GetMasterVolumeLevelScalar(...)`. Save as `original_volume: f32`.
   - Ramp endpoint volume from `original_volume` → `0.0` over 200 ms (20 × 10 ms).
   - For each session in `paused_sessions`: `session.TryPauseAsync()?.get()?`. Errors per-session are logged and skipped (don't bail the whole flow).
   - Snap endpoint volume back to `original_volume` (paused sessions are silent).
   - If `paused_sessions.is_empty()`: no session pause happened → keep endpoint at 0 (mute fallback) and set `is_muted = true`.
   - Store `paused_sessions`, `original_volume`, `is_muted` on `self`.
   - Return `true` if either path engaged.
2. `resume_now` flow:
   - Lock state. For each session in `paused_sessions`: `session.TryPlayAsync()?.get()?` — same per-session error tolerance.
   - If `paused_sessions.is_empty()` AND `is_muted`: ramp endpoint from current → `original_volume` over 200 ms (mute fallback path).
   - Else (sessions resumed): snap endpoint to 0, then ramp 0 → `original_volume` over 200 ms (so the resumed audio fades in).
   - Clear state.
3. Serialize fade ops with a mutex on the controller's state.
4. WinRT type imports — confirm `windows = { version = "X", features = ["Windows_Media_Control", "Windows_Foundation", ...] }` aligns with the version Tauri's transitive dep already pulls in. If a version mismatch comes up, either bump or hold a separate WinRT instance — Cargo's feature unification usually makes this a non-issue.
5. Manual smoke matrix on Windows:
   - **Spotify desktop playing → record** → SMTC `TryPauseAsync` succeeds, fade-out + pause. Stop → resume + fade-in.
   - **Edge tab playing YouTube → record** → Edge media-session session pauses (Chromium registers SMTC).
   - **Both Spotify + Edge tab playing → record** → BOTH pause (key Windows-vs-Mac difference: SMTC iterates all sessions, MediaRemote only hits the focused one).
   - **A game with no SMTC + Notepad with no audio → record** → no sessions; mute fallback engages; record-end restores volume.
   - **Cancel mid-recording (Esc)** → resume both apps.
   - **Setting=off** → no audio change.
6. `cargo check --target x86_64-pc-windows-gnu` clean.

**Outcome ACs (Backlog).**

- `WindowsMediaController::pause_now` enumerates SMTC sessions, pauses every `Playing` one.
- Endpoint-volume fade-out runs before pause-send, snap-back after pause.
- Mute fallback engages when no Playing sessions exist.
- `resume_now` resumes every previously-paused session; mute fallback path ramps endpoint back up.
- Per-session errors logged but don't bail the whole flow.
- Manual smoke matrix passes for Spotify, Edge/YouTube, multi-app, no-session, cancel-mid-recording, setting=off.

---

### Task 5: GeneralPane "Audio" row + usePauseAudio hook

**Goal.** Add an "Audio" section row in General pane (or extend whichever Behavior/Recording section the executor finds when reading `general-pane.tsx`). One Switch labeled "Pause audio during dictation," wired through a new `usePauseAudio()` hook that mirrors `useShowInFullscreen()`.

**Files.** `apps/tauri/src/lib/use-pause-audio.ts` (new), `apps/tauri/src/components/general-pane.tsx` (extend).

**Steps.**

1. New `apps/tauri/src/lib/use-pause-audio.ts`:
   ```ts
   import { useEffect, useState } from "react";
   import { invoke } from "@tauri-apps/api/core";
   import { listen, type UnlistenFn } from "@tauri-apps/api/event";

   export function usePauseAudio() {
     const [enabled, setEnabledState] = useState(true);

     useEffect(() => {
       invoke<boolean>("behavior_get_pause_audio_during_dictation")
         .then(setEnabledState)
         .catch(() => setEnabledState(true));
     }, []);

     useEffect(() => {
       let unlisten: UnlistenFn | undefined;
       void listen<boolean>("behavior_pause_audio_changed", (e) =>
         setEnabledState(e.payload),
       ).then((fn) => (unlisten = fn));
       return () => unlisten?.();
     }, []);

     const setEnabled = (next: boolean) =>
       invoke("behavior_set_pause_audio_during_dictation", { enabled: next });

     return { enabled, setEnabled };
   }
   ```
   Default state is `true` (matches the Rust-side default) so the Switch starts in the right position before the first invoke resolves.
2. Read current `general-pane.tsx`. If a Behavior or Audio section already exists, add the row there. Otherwise add a new `<Separator />` + `<SectionHeader>Audio</SectionHeader>` block above the Updates section.
3. Add the row using existing shadcn primitives:
   ```tsx
   <Field orientation="horizontal">
     <FieldContent>
       <FieldLabel htmlFor="pause-audio">Pause audio during dictation</FieldLabel>
       <FieldDescription>
         Pauses Spotify, browser playback, and other media when you start recording,
         then resumes when recording ends. Falls back to muting system output for
         apps that don't support media controls.
       </FieldDescription>
     </FieldContent>
     <Switch
       id="pause-audio"
       checked={enabled}
       onCheckedChange={setEnabled}
     />
   </Field>
   ```
4. `pnpm tsc --noEmit` clean from `apps/tauri/`.

**Outcome ACs (Backlog).**

- New `use-pause-audio.ts` hook uses invoke + listen, matches the project's existing Settings hook pattern.
- General pane has a Switch row in an Audio (or merged) section with the spec's description copy.
- Toggling the Switch persists, updates the cache, and is reflected back via the listen subscription.
- `pnpm tsc --noEmit` clean.

---

### Task 6: Playwright spec + tauri shim stubs

**Goal.** Cover the React side of the wiring. Mock `behavior_get_pause_audio_during_dictation` / `behavior_set_pause_audio_during_dictation` at the shim boundary; assert initial state, write-through, and external-event update via `behavior_pause_audio_changed`.

**Files.** `apps/tauri/tests/settings-window.spec.ts` (extend), `apps/tauri/tests/fixtures/tauri-shim.ts` (add stubs + helper).

**Steps.**

1. In the tauri shim, add to the invoke handler:
   - `behavior_get_pause_audio_during_dictation` → returns `window.__owPauseAudio ?? true`.
   - `behavior_set_pause_audio_during_dictation` → writes payload to `window.__owPauseAudioLastSet` and to `__owPauseAudio` so subsequent reads are consistent.
   - Helper `emitPauseAudioChanged(page, value)` that dispatches the event.
2. Add to `test.describe("settings view", ...)`:
   - **"Pause audio Switch reflects behavior_get on mount"** — set the shim default to `false`, open Settings, assert Switch is unchecked.
   - **"Toggling the Switch invokes behavior_set with the new value"** — start checked (default), click, assert `__owPauseAudioLastSet === false`.
   - **"behavior_pause_audio_changed event updates the Switch"** — open Settings, emit the event with `false`, assert the Switch becomes unchecked.
3. Verify the existing settings tests + Theme + section structure tests + Show-in-fullscreen tests still pass.
4. `pnpm test:ui` green (per the CLAUDE.md verification rule — Playwright suite must actually run, don't infer from reading).

**Outcome ACs (Backlog).**

- Tauri shim exposes stubs for the two pause-audio commands plus an `emitPauseAudioChanged` helper.
- Three new tests assert: initial state from `behavior_get_pause_audio_during_dictation`, write-through via `behavior_set_pause_audio_during_dictation`, external event update via `behavior_pause_audio_changed`.
- Existing Settings + General-pane + show-in-fullscreen tests still pass.
- `pnpm test:ui` green locally.

---

## Reviewer loop

After all 6 plan tasks have matching Backlog subtasks (`TASK-61.1` through `TASK-61.6`), dispatch a reviewer agent (general-purpose, with explicit criteria — there is no dedicated reviewer subagent installed) with:

- The standard plan-quality criteria (bite-sized tasks, verifiable outcome ACs, concrete file paths, test/verification per task, no deferred design decisions, ordering + dependencies explicit).
- The verbatim Backlog-enforcement fragment from `.claude/skills/writing-backlog-plans/references/plan-reviewer-addendum.md`.
- One project-specific extra check: that **the phase observer wiring in Task 2 does NOT also re-fire `pause_now` on PHASE_LOADING_MODEL → PHASE_RECORDING during a model download** if the model finishes loading mid-record-attempt — that path enters PHASE_RECORDING from LOADING_MODEL not IDLE, and the observer must still treat it as "entry" (single edge: prev != RECORDING && current == RECORDING covers both).

## Execution handoff

Sequential where there is a dependency, parallel where not:

- Task 1 first (schema + cache + commands).
- Task 2 next (trait + observer wired to stubs).
- Tasks 3 and 4 can land in parallel on different machines (Mac box runs Task 3, Windows box runs Task 4 — see `openwhisper-releases` skill for the existing two-machine split).
- Task 5 depends on Task 1's commands existing but doesn't need Tasks 2/3/4 to run on either platform.
- Task 6 depends on Task 5.

Status updates flow through `backlog task edit` per the cheatsheet. Each subtask appends commit refs in implementation notes (`--append-notes`), checks ACs as they land (`--check-ac`), and ends with a `--final-summary` + `-s Done`.

## TDD shape note

Task 1 is straight schema + glue; cargo check + a small persistence test covers it. Task 2 is integration-shaped (observer wired into emitter loop); manual smoke with the no-op stubs is the pragmatic verification. Tasks 3 and 4 are platform-specific OS integration; the smoke matrices in each task body are the verification — unit tests against MediaRemote / SMTC are not feasible without a real audio environment. Task 5 leans TDD with Task 6 — write the React hook, then the tests, iterate until green.

The phase-transition logic in Task 2 deserves a `cargo test` covering the four state transitions (idle→recording, recording→transcribing, recording→idle-via-cancel, recording→error) and asserting `pause_now` / `resume_now` fire exactly once per recording session — using a mock `MediaController` whose calls are counted.
