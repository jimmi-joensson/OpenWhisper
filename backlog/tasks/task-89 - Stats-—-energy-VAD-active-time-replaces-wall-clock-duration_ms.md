---
id: TASK-89
title: Stats — energy-VAD active-time replaces wall-clock duration_ms
status: In Progress
assignee:
  - '@claude'
created_date: '2026-05-06 09:30'
updated_date: '2026-05-06 09:31'
labels: []
dependencies: []
ordinal: 60000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Wall-clock recording duration overcounts when the user leaves the mic on with no speech (today the React-side cap at 100 wpm hides the worst of it). Replace duration_ms in the dictations table with active-speech-ms computed from the raw f32 samples in core/src/audio.rs via a 20 ms RMS frame + threshold. Model-agnostic — works regardless of which recognizer transcribes. Lets us drop the React cap.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 core::audio exposes estimate_voiced_ms(samples: &[f32], sample_rate: u32) -> i64
- [ ] #2 Threshold and frame size are constants in audio.rs (no settings UI in this task); 20 ms frame, RMS dBFS threshold ≈ -40 dB
- [ ] #3 dictation_deliver_transcript captures voiced_ms (via dictation state field set by the shell after audio drain) and stats::record_dictation receives voiced_ms in place of wall-clock duration
- [ ] #4 Apps/tauri shell calls audio::estimate_voiced_ms after audio_drain_samples and pushes the result via a new dictation::dictation_set_voiced_ms entry point
- [ ] #5 stats-strip.tsx drops the SPEAKING_CEILING_WPM cap once landed (or keeps it as a safety net pending user smoke)
- [ ] #6 Unit tests: silence-only samples → 0 voiced_ms; pure tone above threshold for 1 s → ~1000 voiced_ms; mixed silent+voiced → only voiced frames counted
<!-- AC:END -->
