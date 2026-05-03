---
id: TASK-61.3
title: 'Plan Task 3: macOS MediaController implementation'
status: To Do
assignee: []
created_date: '2026-04-30 22:18'
labels:
  - 61-impl
dependencies: []
parent_task_id: TASK-61
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 MacMediaController resolves MediaRemote via dlopen/dlsym; falls back to mute-only when symbols absent
- [ ] #2 Pause flow ramps endpoint to 0, sends kMRPause, snaps endpoint back to original
- [ ] #3 Resume flow snaps endpoint to 0, sends kMRPlay, ramps endpoint up to original over 200ms
- [ ] #4 Mute fallback ramps to 0 on pause and back to original on resume
- [ ] #5 State protected by mutex; rapid toggle does not queue overlapping fades
- [ ] #6 Smoke matrix passes: Spotify, Safari/YouTube, no-media-app, cancel-mid-recording, setting=off
<!-- AC:END -->

## Implementation Notes (2026-05-03 release-blocker rework + revert)

ACs above are stale — original v0.5 plan was MediaRemote `kMRPause`/`kMRPlay` + endpoint mute-fade. Shipped `0.5.0` uses **AppleScript-per-app** (`tell application "Spotify"`/`"Music"`) which is what's in tree right now.

### What shipped in `0.5.0`
- AppleScript synchronous `if player state is playing then pause` for Spotify + Music. Returns deterministic "did this app actually pause" signal so resume only plays back exactly what we paused.
- Best-effort `kMRPause` (opcode 1) sent alongside as a no-op-on-paused fallback for browser tabs. We deliberately do NOT send the matching `kMRPlay` on resume — that would resume externally-paused apps and reintroduce the "stop with nothing playing → music starts" regression.
- Sample-rate poll on the default-output device so the resume `play` waits for BT switchback (HFP→A2DP) before posting.
- **Documented limitation:** browser-tab media (Safari/Chrome/Firefox) is not paused. Browser tabs expose no AppleScript pause for per-tab media. **Documented limitation:** one-time Automation TCC prompt per AppleScript-driven app on first audio-ducking use (Spotify, Music). Lazy — only fires when the user actually plays those apps during a record.

### What was tried during the v0.5 release-blocker rework + reverted
Three commits today attempted to remove the TCC prompts by replacing AppleScript with synthesized media-key events (`NX_KEYTYPE_PLAY` via `+[NSEvent otherEventWithType:…]` → `CGEventPost(kCGHIDEventTap, …)`). All three were reverted in `<followup-revert-sha>` after smoke testing exposed unfixable edge cases. See revert commit message for the full sequence; summary:

1. **Pure media-key (commit `0e387c6`).** Single toggle. Worked for one playing app. Didn't reach a second app — media keys route to one elected NowPlaying client at a time.
2. **Multi-toggle burst (commit `96a8986`).** Counted active output producers via `kAudioHardwarePropertyProcessObjectList` + `kAudioProcessPropertyIsRunningOutput`, sent N toggles 40 ms apart. Smoke proved `mediaremoted`'s NowPlaying re-election doesn't complete inside any latency budget the user can tolerate (40 ms → 2nd toggle still routes to the just-paused first app).
3. **Hybrid AppleScript-for-Spotify/Music + media-key-for-everything-else (commit `9a7fa66`).** Worked for Spotify-alone, Spotify+browser first iteration. Failed on second iteration (mediaremoted election still races even after a settle delay) and on "manually-paused-browser then record" (background system noise tripped `is_audio_playing` → false-trigger toggle resumed the manually-paused tab).

### Why deterministic multi-app pause is unsolved post-15.4
Apple gated the entire `MediaRemote.framework` — both `MRMediaRemoteSendCommand` SET and `MRMediaRemoteGetNowPlaying*` READ — behind a `com.apple.*` entitlement no third-party app holds, starting macOS 15.4 ([BTT thread](https://community.folivora.ai/t/now-playing-is-no-longer-working-on-macos-15-4/42802)). The only working post-15.4 path is the [ungive/mediaremote-adapter](https://github.com/ungive/mediaremote-adapter) `/usr/bin/perl`-bridge hack — well past "non-hacky" for a v0.5 nicety. BetterTouchTool (most-shipped niche player) also stays on AppleScript-per-app post-15.4. Iteration-budget rule fired at attempt #4; correct call was to back out.

### Follow-up
Tracked separately for post-0.5 exploration: deterministic multi-app pause covering browser tabs would need either (a) the Perl-bridge approach productionised, (b) the framework re-opened by Apple, or (c) a per-browser DevTools-protocol path (VS-Code-style attach to Chrome/Safari debug ports — heavy, fragile). None feasible inside v0.5's release window. Open a TASK-XX after release if the limitation actually bites users.
