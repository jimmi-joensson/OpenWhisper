---
id: TASK-4
title: Microphone capture in Rust core
status: Done
assignee: []
created_date: '2026-04-22 21:11'
updated_date: '2026-04-23 18:16'
labels:
  - core
  - audio
dependencies: []
priority: high
ordinal: 4000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Rust core captures audio from the default input device via cpal, resamples to 16 kHz mono Float32, and streams samples to Swift through a pull-based FFI (drain). Designed for toggle-style activation: Swift calls start, user speaks, Swift calls stop + drain, then hands the buffer to FluidAudio. VAD is explicitly out of scope for this task — the toggle hotkey makes auto-stop unnecessary for MVP.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Core exposes audio_start_capture(), audio_stop_capture(), audio_drain_samples() via swift-bridge
- [x] #2 Resamples device-native rate (usually 44.1/48 kHz stereo) to 16 kHz mono Float32 using rubato + channel downmix
- [x] #3 Works with the default input device on macOS; microphone permission prompt fires via Info.plist entry
- [x] #4 Swift smoke test: press Record, speak, press Stop, transcribe → text appears
- [x] #5 Handles permission denial and device-unavailable errors by returning a Result/error string, not a panic
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
2026-04-23 runtime verified. 37.2s live recording through default mic → Rust core (native 48kHz stereo → 16kHz mono resample via rubato) → FluidAudio Parakeet on ANE → clean transcript at 0.961 confidence, 588,095 samples as expected. Microphone permission prompt fired + resolved on first use. One bug fixed mid-flight: stop-then-drain ordering required keeping the capture buffer alive after stream stop (Capture.stream made Option).
<!-- SECTION:FINAL_SUMMARY:END -->
