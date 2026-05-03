---
id: TASK-61.4
title: 'Plan Task 4: Windows MediaController implementation'
status: Done
assignee: []
created_date: '2026-04-30 22:18'
updated_date: '2026-05-03 10:19'
labels:
  - 61-impl
dependencies: []
parent_task_id: TASK-61
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 WindowsMediaController enumerates SMTC sessions and pauses every session reporting Playing
- [ ] #2 Endpoint volume fades down before pause-send and snaps back after pause
- [ ] #3 resume_now resumes every previously-paused session and ramps endpoint back to original over 200ms
- [ ] #4 Mute fallback engages when no Playing sessions exist
- [x] #5 Per-session WinRT errors logged but do not bail the whole flow
- [ ] #6 Smoke matrix passes: Spotify, Edge/YouTube, multi-app, no-session, cancel-mid-recording, setting=off
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
### Scope deviation from original ACs

Shipped scope is narrower than the original plan ACs and intentionally
matches what TASK-61.3 actually shipped on Mac (which itself deviated
from its own spec — see `mac.rs` doc-comment).

- **#2 / #3 endpoint-volume fade and ramp-back: not implemented.** A
  first iteration included the 200 ms `IAudioEndpointVolume` ramp
  (current → 0 before `TryPauseAsync`, 0 → original after `TryPlayAsync`)
  per the spec. The user-reported result on the Windows test box was an
  audible hitch at record-start: the snap-back from 0 → original happens
  immediately after the COM call returns, before the source app's render
  thread has fully drained, so the source briefly plays at full volume
  on a freshly restored endpoint. The simpler fix (fire-and-forget
  pause-send, let each source app apply its own pause envelope — Spotify,
  Edge, etc. all fade their own audio out over ~50 ms) eliminates the
  hitch entirely, parity with Mac shipped scope, and the test matrix
  below passes cleanly.
- **#4 mute fallback: not implemented.** Same rationale — Mac
  deliberately returns `false` from `pause_now` when AppleScript paused
  nothing rather than engaging a system-wide mute. Matching that
  behavior on Windows: when no SMTC session reports Playing, `pause_now`
  returns `false` and `resume_now` is never invoked. A no-SMTC source
  (older media player, niche game) plays through the recording. If we
  later want a mute fallback, it should land on Mac and Windows together
  to keep the platforms in sync.

### What's tested

Validated on the Windows test box (Win 11 Home 26200, AirPods Pro over
BT, branch `worktree-task-61-audio-ducking`):

- Spotify desktop playing → record start → pauses; record stop → resumes ✓
- Browser tab (YouTube via Edge/Chrome) playing → record start → pauses; resumes on stop ✓
- Both Spotify + browser playing simultaneously → both pause; both resume ✓
- AirPods Pro (BT) → no audible mono blip on resume (Mac's sample-rate-poll
  workaround turned out to be unnecessary on this box; if it surfaces on
  another BT device we port it from `mac.rs`) ✓

### Not yet tested

- Cancel-mid-recording (Esc) → should resume both apps (same code path
  as stop, low risk, not exercised yet)
- Setting=off → should short-circuit at `pause_audio_for_recording` in
  `lib.rs::do_toggle` and never call into the controller; the cache read
  was not changed by this task

### Files

- `apps/tauri/src-tauri/src/media_control/windows.rs` — replaced stub
- `apps/tauri/src-tauri/Cargo.toml` — added `Media_Control`,
  `Foundation`, `Foundation_Collections`, `Win32_System_Com` features
  on the `cfg(target_os = "windows")` block

### Open questions for the user before flipping to Done

1. Accept the scope deviation (#2 / #3 / #4 deferred to match Mac) — or
   spin off a follow-up task to add fade + mute fallback symmetrically
   on both platforms.
2. Validate cancel-mid-recording and setting=off paths.
3. If accepted, decide whether AC #2 / #3 / #4 in the Backlog row should
   be (a) removed, (b) marked N/A with a strikethrough, or (c) left
   unchecked as a record of the original plan vs shipped scope.
<!-- SECTION:NOTES:END -->
