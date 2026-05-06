---
id: TASK-89
title: Stats — energy-VAD active-time replaces wall-clock duration_ms
status: Done
assignee:
  - '@claude'
created_date: '2026-05-06 09:30'
updated_date: '2026-05-06 19:03'
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
- [x] #1 core::audio exposes estimate_voiced_ms(samples: &[f32], sample_rate: u32) -> i64
- [x] #2 Threshold and frame size are constants in audio.rs (no settings UI in this task); 20 ms frame, RMS dBFS threshold ≈ -40 dB
- [x] #3 dictation_deliver_transcript captures voiced_ms (via dictation state field set by the shell after audio drain) and stats::record_dictation receives voiced_ms in place of wall-clock duration
- [x] #4 Apps/tauri shell calls audio::estimate_voiced_ms after audio_drain_samples and pushes the result via a new dictation::dictation_set_voiced_ms entry point
- [x] #5 stats-strip.tsx drops the SPEAKING_CEILING_WPM cap once landed (or keeps it as a safety net pending user smoke)
- [x] #6 Unit tests: silence-only samples → 0 voiced_ms; pure tone above threshold for 1 s → ~1000 voiced_ms; mixed silent+voiced → only voiced frames counted
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Commit: 31764d2. cargo test -p openwhisper-core --lib → 41/41 (5 new vad_tests). cargo check -p openwhisper-tauri clean. tsc + 82/82 pw green. Awaiting Rust rebuild via dev-run.sh + manual smoke (record + leave mic running silent, watch Time Saved hold steady).

31764d2 TASK-89: energy-VAD landed; cap removed; 5 vad tests + 41 lib tests green
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Merged via PR #16 (squash 2415f3a). Cross-platform smoke green: Mac + Windows.
<!-- SECTION:FINAL_SUMMARY:END -->
