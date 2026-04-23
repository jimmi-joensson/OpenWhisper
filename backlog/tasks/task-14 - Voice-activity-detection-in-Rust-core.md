---
id: TASK-14
title: Voice activity detection in Rust core
status: To Do
assignee: []
created_date: '2026-04-23 05:57'
labels:
  - core
  - audio
dependencies:
  - TASK-4
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Follow-up to TASK-4: stream-based VAD that emits speech segment boundaries. Enables auto-stop (no explicit hotkey-to-end), push-to-silence UX, and silence-trimming before transcription. Out of MVP; useful once we want continuous-listening modes.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Silero VAD (ONNX) or webrtc-vad integrated into the cpal callback path
- [ ] #2 VAD segments emitted with start/end timestamps through FFI
- [ ] #3 Sensitivity configurable from Swift settings
<!-- AC:END -->
