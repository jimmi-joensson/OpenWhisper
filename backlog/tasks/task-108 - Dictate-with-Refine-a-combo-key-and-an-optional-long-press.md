---
id: TASK-108
title: 'Dictate with Refine: a combo key and an optional long press'
status: In Review
assignee: []
created_date: '2026-11-25T11:51:42Z'
labels:
  - tauri
  - hotkey
  - refine
dependencies: []
priority: medium
ordinal: 106000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Asked for on 2026-11-25: a way to get one dictation refined without switching Refine on. The plain dictation key keeps following the Refine switch.

Two gestures, both ending in the same core path (dictation::run_toggle_refined):
- A new optional hotkey slot, refine. macOS default: Right Command and Right Option together (a new binding kind, modifier-combo: two or more modifier keys held together and let go). No default on Windows; the user records a chord.
- An opt-in long press of the dictation key (hotkeys.long_press = refine, off by default, 400 ms). Only for a dictation key that is a single modifier, which fires on release; a chord fires on key down and has no long press. Stored as an action, not a switch, so hold-to-talk can become another choice later.

Works at either end of a recording: start with it, or stop a plain dictation with it. Everything else (backend, mode, instructions, word minimum) is what the Refine settings say. The stored switch is not touched.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The refine key refines a dictation with the Refine switch off, started or stopped with it
- [x] #2 The plain dictation key still follows the Refine switch
- [ ] #3 Right Command plus Right Option fires the combo and neither tap; each key alone is still its tap
- [ ] #4 The slot can be recorded, cleared and reset in Settings, Shortcuts, including a combo of two modifier keys
- [ ] #5 Long press is off by default and, switched on, a hold of 400 ms or more works like the refine key
- [ ] #6 Windows path compiles; run on a Windows machine before leaving In Review
- [x] #7 Tried by hand on macOS with a real Refine backend before leaving In Review
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Branch task-108-refine-key, cut from task-97-ask (the slot table with the ask slot is not on main yet). Nothing committed yet (2026-11-25).

Core: `dictation::run_toggle_refined` / `dictation_request_toggle_refined` mark the dictation (`refine_forced` on the state, the snapshot and the tick); `refine::warm_for_recording_with` and `run_for_dictation_with` take the mark and treat the switch as on for that one run. `HotkeyKind::ModifierCombo`, `HotkeyTarget::Refine`, `HotkeySettings.refine` and `.long_press` (`LongPressAction`, `LONG_PRESS_MS` = 400) in core settings.
Shell: one more entry in `SLOTS`; `hotkey::long_press_action`; mac tap matches combos (`combo_step`), times the tap key for the long press and records combos in capture mode (`capture_step`). Windows refuses a modifier-combo like a modifier-tap, so a settings file from a Mac reports the slot as skipped. Commands: `dictation_toggle_refined`, `settings_set_long_press`, `settings_get_hotkey_defaults` (the pane asks the backend which slots have a default; the window cannot tell the platform reliably, Playwright's Desktop Chrome says Win32 on a Mac).
UI: a "Dictate with Refine" row and a "Long press to refine" switch in Settings, Shortcuts. The switch only shows while the dictation key is a single modifier.

Headless: no CLI change. The CLI has no hotkey surface, and `openwhisper refine run` already refines a text whatever the switch says.

In Review since 2026-11-25. Nothing committed yet: the work is in the working tree of task-108-refine-key.
Checked at the end: core 604 tests pass (tauri features), shell 77, CLI 45, vitest 53, Playwright 329, `cargo check --target x86_64-pc-windows-gnu`.
Tried by hand by the author the same day, in the Dev app with the Claude backend (AC 7): the combo at the start of a dictation, several times, and plain dictations in between. The Dev database and the verbose log agree with what was pressed: every plain dictation has refine_ms=0 and no row in refine_runs (AC 2).
Not tried by hand, covered by tests only: stopping a plain dictation with the combo (AC 1, second half), each key alone next to the combo with Ask on Right Option (AC 3; the author's ask key is Right Control), recording a combo in Settings (AC 4), the long press (AC 5). Nothing was run on Windows (AC 6).

Known, not from this task: `settings::tests::a_reader_never_sees_a_half_written_settings_file` fails when the whole core suite runs (passes alone), also on task-97-ask before this work.

The Refine mark (asked for on 2026-11-25, after trying the keys: "a hard time seeing if a message was refined or not"). One mark for Refine everywhere, the Lucide Sparkles icon with the word "Refined", and no second colour: orange stays "the microphone is live" (identity-tokens.md).
- Pill: "Refine" above the capsule for a moment when a recording starts that will be refined (`will_refine` on the tick: the switch is on, or the refine key or a long press started it). The line stays up for the whole dictation (asked for after trying a version that showed it for a moment): "Refine" while it is recorded and transcribed, "Refining" while the model has the text, gone when the text is delivered. `will_refine` therefore covers the transcribing phase too. A mode name shown meanwhile gives the place back. The capsule itself is unchanged: a first build turned the sphere into a sparkle while refining, tried on the real pill and taken out the same day (the usual animation, with the line floating above it).
- Rows: the latest transcript says "Refined by Sonnet 5.5 at low effort". History rows say "Refined · Sonnet 5.5 · Low" in the small print next to the time and the app (a badge in the first build; plain text was asked for). The model is the exact one the backend names (`CallUsage.model`, from the CLI's `modelUsage` or the server's `model`), else the name in the settings, else the backend. A stored entry keeps it on its extra line (`refined=claude-sonnet-5-5; effort=low`), and an entry from before the fields shows "Refined" when it has an original.
- Shortcuts: the mark in front of "Dictate with Refine" and "Long press to refine".
- Home: the Refine card says what Refine would use, switch on or off: "Sonnet 5.5 · Low" (the effort as the Refine settings name it; the word "effort" was dropped from the small print as not needed) (`refine::model_in_use`, command `refine_model_in_use`: backend, model and effort of the selected mode). The settings only have a loose name ("sonnet"); which model that is comes from the stored runs, so each run now keeps the exact model the backend named (`refine_runs.model_exact`, migration 7; also in `openwhisper refine history --json`). Before the first run with this build the card shows the loose name.
- Not on the pill: model and effort while dictating were considered and left out. The pill is glanced at dozens of times a day and the answer is the same every time; the exact model is also only known after the call.
Core test `with_the_switch_off_only_a_dictation_marked_by_the_refine_key_reaches_the_backend` runs the stop pipeline's Refine hooks with a counting backend: plain dictations build, warm and ask nothing, also right after a forced one. Checked on the Dev database the same day: 6 dictations after the first build, 1 Refine run, the one made with the combo.

Slowness seen while trying the Dev build (2026-11-25), from its verbose log: on plain dictations refine_ms=0 and the time after the stop key is drain_ms (resampling), which grows with the recording: 98 ms for 2 s of speech, 1101 ms for 21 s, 1996 ms for 39 s; transcribe_ms 66 to 240. The Dev app is an unoptimized build. Not compared against a release build of the same code.
The verbose log now has a line per dictation saying why Refine did or did not run (`[ow.refine] dictation switch_on=… refine_key=…`).

Open: with a long press there is still no sign before the key is let go. On the Home card the line has a row of its own under the text and the controls (it wrapped in two inside the narrow text column at first, which was hard to read).
<!-- SECTION:NOTES:END -->
