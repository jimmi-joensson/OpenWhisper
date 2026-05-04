---
id: TASK-82
title: Surface AppleScript Automation TCC denial in pause-during-dictation
status: In Review
assignee: []
created_date: '2026-05-04 18:55'
updated_date: '2026-05-04 18:55'
labels: []
dependencies: []
priority: medium
ordinal: 35000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
User report (2026-05-04): "music keeps playing on built-in speakers when I start a recording, but pauses fine on AirPods." Root cause traced to Automation TCC denial for `OpenWhisper → Spotify` — AppleScript `tell application "Spotify" to pause` returns AppleEvent error -1743 ("Not authorized to send Apple events") which the script's per-app `try ... end try` block silently swallowed. AirPods case happens to mask the failure: BT mic open forces HFP profile, OS interrupts A2DP audio, music "stops" via OS-level routing change rather than via AppleScript pause. Built-in speakers have no such mask, so the silent fail surfaces as "feature broken on this output device" — which is what the user reported, conflating output-device with the actual variable (TCC grant state).

The pause path is and always has been output-device-agnostic. The bug is that TCC denial fails silently with no diagnostic — same code is correct on every output, but when AppleScript can't dispatch the pause command we have no surface that tells the user (or us in support) what's wrong.

Companion task TASK-77 covers post-15.4 multi-app + browser-tab pause limits — this task is strictly about making the existing AppleScript-only path's failure mode observable instead of silent.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] AppleScript pause script captures per-target AppleEvent error codes via `on error errMsg number errNum` (replaces the bare `try ... end try` that was silently swallowing -1743)
- [x] `pause_now` parses `paused_apps||error_codes` script output and classifies the failure (`NotAuthorized` when any target returned -1743, `NoKnownPlayer` when nothing was playing, `Other` for everything else)
- [x] On `NotAuthorized`, an actionable message is logged to stderr unconditionally (not gated on `OPENWHISPER_VERBOSE`) so production users / support reviewing Console.app see it: `[media_control.mac] pause_now: Automation permission denied for one or more music apps (...). Grant in System Settings → Privacy & Security → Automation → OpenWhisper.`
- [x] Diagnostic state stored in process-wide `LAST_PAUSE_DIAGNOSTIC: Mutex<Option<PauseDiagnostic>>`, exposed via `media_get_last_pause_diagnostic` Tauri command for future UI consumption
- [x] Cross-platform `PauseDiagnostic` struct in `media_control/mod.rs` with stable string `reason` tag (`not_authorized` / `no_known_player` / `other`); Windows returns `None` (SMTC isn't subject to per-app TCC, no equivalent silent-fail mode)
- [x] Happy path unchanged — when AppleScript pauses successfully, behavior is byte-identical to pre-change
- [ ] Verified end-to-end in `pnpm dev:tauri`: pause + resume work on built-in speakers when Automation is granted; verbose log shows the eprintln line when grant is missing
<!-- AC:END -->

## Implementation Plan
<!-- SECTION:PLAN:BEGIN -->
1. `media_control/mac.rs` — rewrite `build_pause_script` to use `on error errMsg number errNum` per target, append `||<error_codes>` to script output.
2. `media_control/mac.rs` — extend `pause_now` to split-parse the new format, classify into `PauseFailureReason`, write to `LAST_PAUSE_DIAGNOSTIC`, eprintln on `NotAuthorized`.
3. `media_control/mod.rs` — add `PauseDiagnostic` (serde::Serialize) + `last_pause_diagnostic()` cross-platform fn (no-op on Windows).
4. `lib.rs` — register `media_get_last_pause_diagnostic` Tauri command.
5. Verify in dev build: trigger recording with Spotify playing on built-in speakers, confirm pause + resume work (Automation already granted), confirm verbose log shows the unconditional eprintln when grant is absent (test path: revoke Automation in System Settings → repro).
<!-- SECTION:PLAN:END -->

## Out of scope (follow-ups)

- **Startup TCC pre-flight**: at first app launch with `pause_audio_during_dictation` enabled, run a benign no-op AppleScript against each target so Mac shows the Automation grant dialog up-front rather than mid-recording. Worth doing — file as separate task if/when it becomes a usability ask.
- **UI banner in pill / settings** when `last_pause_diagnostic.reason == "not_authorized"` with a "Open Automation Settings" button. Data is already exposed via the new Tauri command; the React side has no consumer yet.
- **Apple Music + Podcasts + browser tabs** coverage stays under TASK-77 per-15.4 limitations. This task only touches the diagnostic layer.

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
The AirPods-vs-built-in confusion is itself a useful artifact to capture: a class of bug where the OS hides a silent fail when one path (BT) has a side effect that mimics the intended behavior, and only surfaces when the side effect is absent (built-in). Worth keeping in `openwhisper-platform-gotchas` if the pattern recurs.
<!-- SECTION:NOTES:END -->
