---
id: TASK-79
title: >-
  Fix periodic 2s level-stream stall — move audio device enumeration off emitter
  thread
status: In Progress
assignee: []
created_date: '2026-05-04 05:44'
updated_date: '2026-05-04 06:28'
labels: []
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Soundbar + pill animations stall periodically every ~2s during dictation and mic-test ("all bars freeze together for a moment, then resume"). Originally scoped as a day-long dual-end instrumentation spike (RAF + audio-callback latency). A 30-minute pre-investigation (2026-05-04) located the cause — scope collapses to a small, targeted fix.

**Both surfaces are the same bug.** The mic-test meter in `apps/tauri/src/Settings.tsx:241-270` reads `dictation.levels` / `dictation.level` from the same `dictation_tick` event the recording soundbar consumes (comment at lines 241-248 calls this out explicitly). One emitter thread, one stall, two visible symptoms. Fixing the emitter thread fixes both.

### Root cause (confirmed by code reading)

In `apps/tauri/src-tauri/src/lib.rs`, the dictation emitter runs a single thread that:

- Sleeps `TICK_MS = 50ms` (lib.rs:107, lib.rs:439)
- Each tick, emits `dictation_tick` carrying the audio level for the soundbar (lib.rs:465)
- Every `DEVICE_STATE_TICK_DIVISOR = 40` ticks (i.e. **2000ms exactly**, lib.rs:420, lib.rs:468) calls `compute_audio_device_state()` (lib.rs:255) → `audio::audio_list_input_devices()` (lib.rs:271) → full cpal device enumerate, which on macOS does per-device CoreAudio I/O queries

Device enumeration runs on the same thread as the level-tick emit. Whenever the enumerate takes longer than ~50ms, the next `dictation_tick` is delayed by that amount → the UI receives no level updates for the duration → all bars hold their last value → visible stutter at exactly 2s cadence.

### Hypotheses now ruled out

- **Streaming recognizer chunk boundary**: there is no streaming recognizer in core. `recognizer_transcribe()` is a one-shot batch call after stop (`core/src/recognizer/mod.rs:86-99`); no per-chunk step runs during recording.
- **Audio sample drop / missing words from this stall**: audio capture runs on the cpal callback thread and accumulates into a buffer independently of the emitter (`core/src/audio.rs:282-314`). The 2s stall pauses UI level updates only; samples are not lost. **Missing-words is therefore a separate problem** (likely a Parakeet-TDT model quirk — see follow-up task) and is explicitly out of scope here.
- **RAF starvation / React re-render**: emitter-thread stall starves the level-event stream itself, so the UI side is innocent. RAF instrumentation is no longer needed to confirm.

### Fix direction (decide during implementation)

Decouple device enumeration from the level-tick emit. Options to evaluate:

- Run `compute_audio_device_state()` on a dedicated worker thread/task at its own 2s cadence; emitter reads cached result lock-free.
- Or: keep the cadence in the emitter but spawn the cpal call on a `tokio::task::spawn_blocking` (or std `thread::spawn`) and drop the result through a channel; emitter never blocks on it.
- Either way: the level emit must never wait on audio device IO.

Pick the simpler of the two that doesn't require restructuring the dictation tick payload shape.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Verify: instrument emitter thread temporarily to log per-iteration wall-clock duration; confirm enumerate ticks consistently exceed 50ms on macOS and account for the observed stutter. Capture one trace as evidence in the PR description.
- [ ] #2 Refactor: move `audio_list_input_devices()` off the emitter thread. Emitter must never call IO that can block longer than a single tick budget (~10ms target).
- [ ] #3 Audio-device-state event continues to fire at ~2s cadence (no regression in device hot-plug detection latency).
- [ ] #4 Manual repro: record continuous steady speech for 30s on macOS; recording-pill soundbar shows no visible stutter at 2s cadence. Then repeat in the Settings → Audio mic-test pane (same `dictation_tick` consumer) — also no stutter.
- [ ] #5 Manual repro on Windows: same — confirm no regression on the platform that wasn't the original culprit.
- [ ] #6 Playwright spec under `apps/tauri/tests/` asserts that `dictation_tick` events arrive with inter-event gap p99 < (e.g.) 100ms over a 10s recording — guards against future re-introductions of blocking IO on the emitter thread.
- [ ] #7 No new permanent debug logging left in release build (verification logging from AC #1 is removed or feature-gated).
<!-- AC:END -->
