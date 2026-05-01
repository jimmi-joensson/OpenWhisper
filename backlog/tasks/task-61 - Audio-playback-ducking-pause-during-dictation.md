---
id: TASK-61
title: Audio playback ducking + pause during dictation
status: To Do
assignee: []
created_date: '2026-04-30 22:12'
updated_date: '2026-05-01 08:13'
labels: []
dependencies: []
documentation:
  - backlog/docs/specs/2026-05-01-audio-ducking-during-dictation.md
  - backlog/docs/plans/2026-05-01-audio-ducking-during-dictation.md
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Pause/duck other apps' audio while OpenWhisper is recording. On record-start: fade out + pause all active media sessions (Spotify, browser tabs, etc); fall back to system-output mute-fade for apps with no media-session API. On record-end (transcribe-phase OR cancel): resume + fade back to prior volume. Single setting in Settings → General/Audio: 'Pause audio during dictation' on/off, default on. Cross-platform: Mac via MediaRemote/MPNowPlayingInfoCenter + CoreAudio, Windows via SMTC GlobalSystemMediaTransportControlsSessionManager + Core Audio IAudioSessionControl2. Driver-detection (BT mono profile vs multi-driver headsets) deferred — uniform behavior in v1.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Setting 'Pause audio during dictation' lands in Settings → General with default on
- [ ] #2 On record-start with setting=on, currently-playing media-session apps fade out and pause
- [ ] #3 On record-end (transcribing or cancel) with setting=on, paused apps resume and fade back to prior volume
- [ ] #4 Apps without media-session API are handled by system-output mute-fade fallback
- [ ] #5 Setting=off short-circuits all audio control; never touches user audio
- [ ] #6 Mac and Windows both implemented behind a shared trait so core orchestrates both
- [ ] #7 Recording cancel path resumes audio identically to stop path
<!-- AC:END -->
