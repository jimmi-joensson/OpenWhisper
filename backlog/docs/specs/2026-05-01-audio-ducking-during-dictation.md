# Pause audio during dictation — design

**Backlog parent:** TASK-61
**Date:** 2026-05-01
**Status:** Spec → Plan
**Source:** observed Bluetooth-AirPods-style mono-profile shift on record-start; user-confirmed scope on 2026-05-01.

## Problem

When OpenWhisper opens the mic during recording, two things degrade other-app audio playback that the user is currently consuming (Spotify, YouTube, Twitch, podcast app, etc.):

1. **Bluetooth single-driver devices** (AirPods, single-driver headsets) — the OS forces the BT link from A2DP (high-quality stereo, 44.1 kHz) into HFP / Hands-Free Profile (mono, 8/16 kHz) the moment any app activates the mic. Music quality collapses to phone-call quality for as long as recording lasts. There is no per-app override; this is a Bluetooth profile-switch governed by the OS audio stack on both macOS and Windows.
2. **Wired / multi-driver headsets** (e.g. SteelSeries Arctis with separate "chat" + "game" endpoints) — output stays high-quality but the user's own voice is now competing with the music for cognitive bandwidth. Speech composition while music plays is harder than necessary.

User cannot reasonably be expected to manually pause Spotify before every dictation burst. Other dictation apps (Superwhisper, Wispr Flow) handle this automatically; OpenWhisper does not.

## Goal

When recording starts and the user has opted in (default on), pause every other app currently playing audio, fading out first so the cut is musically smooth. When recording ends — whether the user tapped the hotkey to stop (→ PHASE_TRANSCRIBING) or cancelled (→ PHASE_IDLE) — resume those apps and fade volume back up to wherever it was. Resuming during transcription is fine; the user has already moved on by that point.

For apps that don't expose a media-session interface (random Electron apps, games, system sounds, web players that don't register media controls), fall back to fading the **system output endpoint** to mute on record-start and restoring it on record-end. This affects all outputs system-wide, which the user has accepted as the asymmetry cost — better silent music than mono music.

One setting controls everything: **Settings → General → Audio**, "Pause audio during dictation", default **on**.

## Non-goals (this spec)

- **Driver-class detection.** v1 applies the same pause+fade behavior to BT and wired-multi-driver alike. A future revision could detect "default input device == default output device" or "BT HFP profile imminent" and switch to a duck-only mode for multi-driver devices, but the user explicitly accepted uniform behavior to ship v1 sooner.
- **Per-app rules.** No "always pause Spotify, never pause Discord" allowlist. Single global toggle.
- **Per-volume duck slider.** Fixed fade target (0% / muted) for v1. Future setting could add a "duck to N%" slider.
- **Pre-emptive A2DP retention.** Telling macOS / Windows "don't switch this BT device to HFP" is not a thing user-mode apps can do reliably. Pausing the audio is the workaround.
- **Per-app system-output volume on macOS.** Mac doesn't ship per-app volume in the public CoreAudio API; third-party tools (Background Music, SoundSource) hook this via virtual devices, which is out of scope. Mute-fade fallback uses the system-output endpoint.
- **Resume coordination with VAD.** When VAD lands (TASK-14), short pauses inside a dictation burst should NOT thrash music pause/resume. This spec assumes the existing "RECORDING phase entry/exit" boundary; VAD task can layer "ignore pause when phase stays RECORDING but VAD is silent".
- **Wake-from-sleep / device-hotswap edge cases.** If the user yanks BT mid-recording or sleeps the laptop with audio paused, behavior is best-effort; we do not attempt heroic recovery.
- **Tray surface.** No tray menu item for the toggle. Single setting in General pane is enough.

## Per-platform behavior

### macOS

- **Pause/resume of media-session apps** uses the private **MediaRemote** framework — the same framework `playbackcontroller(1)` and `nowplaying-cli` shell out to, and that BetterTouchTool / Hammerspoon use. The two functions OpenWhisper calls:
  - `MRMediaRemoteSendCommand(kMRPlay, nil)` / `MRMediaRemoteSendCommand(kMRPause, nil)` — sends the system-wide media-key Pause/Play through the `Now Playing` registry, hitting whichever app currently owns the now-playing slot (Music, Spotify, Safari/YouTube, Podcasts, IINA, etc.).
  - `MRMediaRemoteGetNowPlayingApplicationIsPlaying(...)` — used at record-start to decide "is there anything playing right now? If no, skip the pause + don't bother resuming on stop."
  - This API is private; we link it via `dlopen("/System/Library/PrivateFrameworks/MediaRemote.framework/MediaRemote")` + `dlsym` rather than a build-time link. App Store distribution is not a constraint (OpenWhisper ships outside MAS, see project principles + TASK-12 Developer ID notes), and notarization does not flag the symbol.
  - Caveat: only the **current now-playing app** receives the command. Two simultaneously-playing apps (rare: e.g. Spotify + a YouTube tab) — we pause whichever is the system "now playing" focus, the other keeps playing. Acceptable for v1.
- **Mute-fade fallback** uses CoreAudio public API — `AudioObjectGetPropertyData` / `AudioObjectSetPropertyData` on the default output device with `kAudioDevicePropertyVolumeScalar` (master channel). Linear ramp from current → 0.0 over 200 ms, sample at ~10 ms intervals (~20 steps). Restore on resume: ramp 0.0 → original over 200 ms.
- **Detecting "media-session app responded"** is best-effort: `MRMediaRemoteGetNowPlayingApplicationIsPlaying` is async (Obj-C block callback). After sending Pause we re-poll the playing flag once; if true → media-session pause didn't take, fall through to mute fallback. If false → success, skip mute. Single-shot detection; no retry storm.
- **Entitlements impact.** None. MediaRemote and CoreAudio default-output access work in our existing entitlements set. No new TCC prompts. No `com.apple.security.audio` change needed.

### Windows

- **Pause/resume of media-session apps** uses the public **SMTC** / `Windows.Media.Control.GlobalSystemMediaTransportControlsSessionManager` WinRT API. Steps:
  - `GlobalSystemMediaTransportControlsSessionManager.RequestAsync()` returns the manager.
  - `GetSessions()` enumerates every app that has registered an SMTC session (Spotify desktop, Edge/Chrome tabs that registered media-session, Groove, etc.).
  - For each session whose `GetPlaybackInfo().PlaybackStatus == Playing`, call `TryPauseAsync()` and remember the session for resume.
  - On resume: call `TryPlayAsync()` on each remembered session.
  - Unlike Mac MediaRemote which only hits the foreground app, SMTC enumerates all sessions — so on Windows we genuinely pause every playing app, not just the focused one. (The user's "all active media sessions" intent is fully satisfied on Windows; on Mac it's a single-app approximation.)
- **Mute-fade fallback** uses Core Audio's session API:
  - `IMMDeviceEnumerator::GetDefaultAudioEndpoint(eRender, eMultimedia)` → default output device.
  - `IAudioEndpointVolume::SetMasterVolumeLevelScalar(...)` ramped over 200 ms.
  - Per-app per-session volume (`ISimpleAudioVolume` on each session) is technically available on Windows, but for parity with the Mac fallback (system-wide) and to keep the fallback path simple, we use the endpoint volume.
- **WinRT bindings.** Use the existing `windows-rs` crate (already a transitive dep via Tauri's Windows-specific code). Required features: `Windows_Media_Control`, `Windows_Foundation`. Add to `apps/tauri/src-tauri/Cargo.toml` under the `cfg(target_os = "windows")` block.
- **Permissions.** SMTC is unrestricted at the API level. `IAudioEndpointVolume` requires no extra elevation. No new manifest declarations needed.

## Setting plumbing

- **Storage:** extend the existing `BehaviorSettings` struct in `apps/tauri/src-tauri/src/settings/mod.rs` with a new field `pause_audio_during_dictation: bool`, default **true**. `#[serde(default)]` so older settings files round-trip.
- **In-process cache:** `AtomicBool` next to the existing `SHOW_IN_FULLSCREEN` cache in `apps/tauri/src-tauri/src/behavior.rs`. Hot path is the dictation phase listener which fires per recording transition; we don't want to hit disk there.
- **Rust commands:** `behavior_get_pause_audio_during_dictation() -> bool`, `behavior_set_pause_audio_during_dictation(enabled: bool) -> Result<(), String>`. Setter persists via the existing `save_behavior_settings`, updates the cache, emits `behavior_pause_audio_changed` with the new boolean.
- **Default value:** **on**. Project principle "zero-config over toggles" wants the auto path to be the right one — most users will benefit from auto-pause; the rare user who dictates over background music intentionally can toggle off once.

## Lifecycle / where the pause/resume gets called

The trigger is dictation phase transitions. Existing infrastructure:

- The dictation core (`core/src/dictation.rs`) emits phase ticks via the snapshot poll; the Tauri shell's `spawn_dictation_emitter` (`apps/tauri/src-tauri/src/lib.rs`) re-emits these as a `dictation_tick` event at ~20 Hz.
- The shell already has phase-keyed reaction code (pill window show/hide, hotkey gating).

For audio control, a new module `apps/tauri/src-tauri/src/media_control/` owns the lifecycle. It defines a `MediaController` trait with `pause_now()` + `resume_now()` methods, has a Mac impl + Win impl behind `#[cfg]`, and is driven by a phase observer that:

- On `phase` transition `IDLE → RECORDING` (or `LOADING_MODEL → RECORDING`): if cache says enabled, call `controller.pause_now()`. Remember "did we pause?" so resume only fires if we actually paused.
- On `phase` transition `RECORDING → TRANSCRIBING` or `RECORDING → IDLE` (cancel): if we paused on entry, call `controller.resume_now()`.
- On any other transition: no-op.

The observer runs on the same thread as the existing dictation emitter loop — adding a `prev_phase` local to detect transitions is one line of bookkeeping. No new threads.

**Why phase-trigger and not hotkey-trigger.** Hotkey press lands in `dictation_request_toggle` which can fail (model still loading, fullscreen suppressing, audio device disconnected). Phase-trigger fires only on the actual `PHASE_RECORDING` entry, which already encodes "we successfully started recording." Same for the inverse — phase exit reliably means "audio capture has stopped."

**Cancel path coverage.** `dictation_request_cancel()` flips phase back to IDLE without going through TRANSCRIBING; the phase-exit observer catches both exits with a single rule ("`prev == RECORDING && current != RECORDING` → resume").

**Setting=off short-circuit.** The cache read happens at phase-enter. If the user toggles the setting *during* a recording, we don't re-evaluate mid-recording — too brittle. The user-visible effect: toggle takes effect on the next dictation. Acceptable.

## Trait shape

```rust
// apps/tauri/src-tauri/src/media_control/mod.rs
pub trait MediaController: Send + Sync {
    /// Called on PHASE_RECORDING entry when setting=on.
    /// Returns true if we paused something (so we know to resume).
    fn pause_now(&self) -> bool;

    /// Called on PHASE_RECORDING exit if pause_now previously returned true.
    fn resume_now(&self);
}

#[cfg(target_os = "macos")]
pub use mac::MacMediaController as PlatformMediaController;

#[cfg(target_os = "windows")]
pub use windows::WindowsMediaController as PlatformMediaController;
```

Each impl owns its own state (e.g. Windows remembers the SMTC sessions it paused, Mac remembers the original output volume for the mute-fade case). The phase observer holds a single `Arc<dyn MediaController>` and calls into it.

## Fade semantics

- **Duration:** 200 ms in / 200 ms out. Long enough to sound intentional, short enough not to delay PHASE_RECORDING perceptibly.
- **Curve:** linear is fine for v1. Equal-power or perceptual curve (cube-root) is a future polish.
- **Step rate:** 10 ms (~20 steps over 200 ms). Below the threshold of staircase-audible artifacts on a clean signal.
- **For media-session pause path:** we do NOT fade the system output (other apps' audio could be sharing it). The pause command goes to the source app, which generally cuts cleanly. To smooth that cut without affecting other audio: ramp the **endpoint volume from current → 0 over 200 ms**, send pause, then **immediately restore endpoint volume to original** (paused audio is silent so the user hears nothing during the snap-back). On resume: snap endpoint to 0, send play, ramp 0 → original over 200 ms. This produces a smooth fade for the paused app at the cost of briefly fading anything else playing on that endpoint — acceptable since the use case is "user is consuming a single piece of audio."
- **For mute-fade fallback path:** ramp endpoint volume from current → 0, hold (audio source still playing through the now-muted endpoint), record dictation, ramp endpoint 0 → original on resume. The audio source advances during the recording — this is a deliberate consequence of the "no media-session API" reality, and matches what user described as the fallback case.

## UI surface

Settings → General pane → new section **"Audio"** (or extend an existing Behavior / Recording section if one already lives there at implementation time — General pane has been growing per TASK-54/55/58, executor reads `general-pane.tsx` and decides).

Row contents:

- `FieldLabel` — "Pause audio during dictation"
- `FieldDescription` — "Pauses Spotify, browser playback, and other media when you start recording, then resumes when recording ends. Falls back to muting system output for apps that don't support media controls."
- `Switch` — bound through a new `usePauseAudio()` hook that mirrors the existing `useShowInFullscreen()` shape: `invoke` for read/write, `listen` for cross-surface broadcast on `behavior_pause_audio_changed`.

No tray surface. No hotkey to toggle. One Switch.

## Risks

- **MediaRemote is a private framework.** Apple has not removed it, but they could in any macOS release. The mute-fade fallback path keeps the feature functional even if MediaRemote disappears — pause becomes mute, which is the user-accepted asymmetry. We log when MediaRemote symbol resolution fails so we know the next time it breaks.
- **SMTC sessions can be stale.** A browser tab that played 10 minutes ago may still expose a session whose `PlaybackStatus` lies. Mitigation: only pause sessions reporting `Playing`; never pause `Paused`/`Stopped` sessions and try to resume them.
- **Endpoint-volume contention.** If two apps simultaneously request the endpoint volume (e.g. user manually adjusts volume keys mid-recording), our restore-on-resume could clobber the user's intent. Acceptable: short window (~recording duration), and the user's volume change is preserved on the next recording (we re-read current at fade-out start).
- **Race between pause-send and mic-open.** Sending the pause command and then immediately opening the mic could still trip the BT HFP switch if the OS sees the mic-open before the music-app pause has propagated. We open the mic anyway (recording can't wait); the BT switch happens; user hears the music quality drop briefly and then silence as the app pauses. ~50–200 ms ugly window. Future polish: hold mic-open by 200 ms post-pause, but that adds latency to "tap-and-talk" which is sacred (project principles).
- **Mid-recording setting toggle.** If user turns the setting off mid-recording, we won't resume on stop (we never paused). If user turns the setting on mid-recording, we won't pause (the train has left). Spec says: setting takes effect next recording. UI does NOT need a "this will take effect on the next dictation" hint; the latency is short enough that no one will notice.
- **Multiple recordings in quick succession.** Tap-tap-tap on hotkey: pause / resume / pause / resume / pause. The fade is 200 ms each direction; if the user dictates for less than 400 ms between toggles, we'd queue overlapping fades. Implementation must serialize fade ops on the controller (a `Mutex` around the fade routine is enough).
- **Apple Silicon vs Intel.** No expected divergence. MediaRemote and CoreAudio behave identically.
- **Windows ARM64.** SMTC + Core Audio are both arch-neutral. No expected divergence.

## References

- Project skill `openwhisper-orchestration-in-rust` — confirms phase-observer logic stays in Rust, not in React. Setting cache + phase observer + trait dispatch all live in `apps/tauri/src-tauri/src/`.
- Project skill `openwhisper-platform-gotchas` — entitlements + TCC notes (no impact for this feature).
- Project skill `openwhisper-project-principles` — "zero-config over toggles" supports default-on; "monetization" not relevant; "tap-not-hold" not affected.
- Sibling pattern (mirror this shape): `apps/tauri/src-tauri/src/behavior.rs`, the fullscreen-toggle subtask tree TASK-58.1 → TASK-58.5 (cache + commands + observer + UI hook + Playwright).
- Existing settings struct: `apps/tauri/src-tauri/src/settings/mod.rs::BehaviorSettings`.
- Existing phase emitter: `apps/tauri/src-tauri/src/lib.rs::spawn_dictation_emitter`.
- Existing Settings UI hook pattern: `apps/tauri/src/lib/use-show-in-fullscreen.ts` (planned in TASK-58.4).
- MediaRemote private framework reference (community): https://github.com/PrivateFrameworks/MediaRemote — symbol list, function signatures, consumer projects.
- Windows SMTC docs: https://learn.microsoft.com/en-us/uwp/api/windows.media.control.globalsystemmediatransportcontrolssessionmanager
