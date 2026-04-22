---
id: TASK-4
title: Audio capture + VAD in Rust core
status: To Do
assignee: []
created_date: '2026-04-22 21:11'
labels:
  - core
  - audio
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Use cpal for mic capture. Silero VAD (ONNX) or webrtc-vad to detect speech segments. Stream 16kHz mono f32 frames to consumers.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Core exposes start_capture / stop_capture via C ABI
- [ ] #2 Frames delivered to callback at 20ms cadence
- [ ] #3 VAD segments emitted with start/end timestamps
- [ ] #4 Works with default input device on macOS
<!-- AC:END -->
