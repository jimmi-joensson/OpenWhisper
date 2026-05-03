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

## Implementation Notes

ACs above are stale — mid-v0.5 release-blocker rework. The original plan (MediaRemote `kMRPause`/`kMRPlay` + endpoint mute-fade) and the v0.5 in-tree AppleScript path were both replaced.

**Current implementation (v0.5 release-blocker fix, 2026-05-03):** synthesize the system play/pause media key (`NX_KEYTYPE_PLAY`) via `+[NSEvent otherEventWithType:…]` → `CGEventPost(kCGHIDEventTap, …)`. Same path BetterTouchTool / Hammerspoon / Caffeine use; the OS treats it identically to a real F8 press, so every well-behaved media source — Spotify, Apple Music, browser-tab media (Safari/Chrome/Firefox), podcast apps — toggles in response. No new TCC prompts: HID-layer posting reuses our existing Accessibility grant.

**Why the rework:** per-app AppleScript triggers an Automation TCC prompt the first time the user runs each `tell application "X"` — release-blocking UX for a v0.5 headline feature. MediaRemote `kMRPause`/`kMRPlay` would side-step the prompt but the macOS 15.4+ `com.apple.*` entitlement check on `MRMediaRemoteSendCommand` makes SET commands unreliable from non-Apple-signed processes.

**Toggle gating** (avoids the "stop with nothing playing → music starts" regression and the "user-started-something-mid-recording → we pause it" regression): probe `kAudioDevicePropertyDeviceIsRunningSomewhere` on the default-output device before each post.
- `pause_now`: only post (and set `did_pause = true`) if device is running.
- `resume_now`: only post if `did_pause` is true AND device is currently NOT running.

State + sample-rate-based BT resume wait kept from the prior implementation; `behavior::bt_resume_delay_ms` still Mac-hidden (Mac uses adaptive sample-rate poll).

**Smoke status:** awaiting manual verification by parent session (6-case matrix in handover). Code-complete but not Done.
