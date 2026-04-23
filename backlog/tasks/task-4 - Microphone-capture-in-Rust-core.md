---
id: TASK-4
title: Microphone capture in Rust core
status: In Progress
assignee: []
created_date: '2026-04-22 21:11'
updated_date: '2026-04-23 05:57'
labels:
  - core
  - audio
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Rust core captures audio from the default input device via cpal, resamples to 16 kHz mono Float32, and streams samples to Swift through a pull-based FFI (drain). Designed for toggle-style activation: Swift calls start, user speaks, Swift calls stop + drain, then hands the buffer to FluidAudio. VAD is explicitly out of scope for this task — the toggle hotkey makes auto-stop unnecessary for MVP.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Core exposes audio_start_capture(), audio_stop_capture(), audio_drain_samples() via swift-bridge
- [ ] #2 Resamples device-native rate (usually 44.1/48 kHz stereo) to 16 kHz mono Float32 using rubato + channel downmix
- [ ] #3 Works with the default input device on macOS; microphone permission prompt fires via Info.plist entry
- [ ] #4 Swift smoke test: press Record, speak, press Stop, transcribe → text appears
- [ ] #5 Handles permission denial and device-unavailable errors by returning a Result/error string, not a panic
<!-- AC:END -->
