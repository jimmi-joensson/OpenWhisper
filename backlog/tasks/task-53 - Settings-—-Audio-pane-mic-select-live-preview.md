---
id: TASK-53
title: Settings — Audio pane + mic select + live preview
status: To Do
assignee: []
created_date: '2026-04-27 15:29'
updated_date: '2026-04-27 15:32'
labels:
  - ui
  - tauri
  - audio
  - settings
dependencies:
  - TASK-49
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement Settings → Audio pane per design (screens.jsx SettingsAudioBoard). Microphone device picker + live-preview level meter (32-bar, same geometry as the main window meter). Persists selected device. v1 scope: device only — no gain/AGC/suppression toggles.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 audio_list_devices Tauri command returns input device list from cpal::Host::input_devices()
- [ ] #2 audio_set_device persists chosen device name; begin_capture() looks up by name, falls back to default if missing
- [ ] #3 audio_preview_start / audio_preview_stop reuse the existing AudioEngine but suppress transcription. Mutually exclusive with active recording
- [ ] #4 32-bar LevelMeter (existing component) renders the live preview while the pane is open
- [ ] #5 KV stats show floor (–55 dBFS), live peak, and reported sample rate
- [ ] #6 Switching devices while previewing reopens the stream cleanly (no stuck level)
<!-- AC:END -->
