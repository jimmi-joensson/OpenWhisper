---
id: TASK-91
title: 'Transcribe audio/video files (pick or drop a file, copy result)'
status: In Progress
assignee: []
created_date: '2026-10-22 10:32'
updated_date: '2026-10-22 10:45'
labels:
  - feature
  - core
  - cli
  - ui
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Users want to transcribe an existing recording (WhatsApp voice note, meeting recording, video) instead of live dictation. Pick or drag-drop a file in the app, see progress, copy the transcript like the Home latest-transcript row.

Parakeet v3 is multilingual (25 EU languages), so non-English files (e.g. Latvian) work. Translation is out of scope.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 core: pub fn decodes common audio/video containers (wav, mp3, m4a/aac, mp4/mov video, ogg/opus incl. WhatsApp .opus, webm/mkv, flac) to 16 kHz mono f32 without needing ffmpeg
- [x] #2 core: long files are split into silence-aligned chunks so memory stays bounded on the ORT path and live dictation can interleave between chunks
- [x] #3 core: single in-flight file job with snapshot (phase, progress, status string, text, error) + cancel; state machine lives in core
- [x] #4 CLI: openwhisper transcribe accepts any supported file (not only 16 kHz WAV), human + --json output
- [x] #5 UI: Transcribe route with Choose file button + drag-and-drop, progress, cancel, result with Copy button
- [x] #6 Playwright suite passes; new spec covers the Transcribe route
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Core: core/src/file_transcription/ (decode.rs symphonia 0.6.1 + pure-Rust opus-decoder for Ogg/WebM Opus; chunk.rs silence-aligned <=30 s chunks; mod.rs transcribe_file + single-job phase machine FILE_PHASE_*, status strings, cancel at chunk boundaries). Deps gated behind the `recognizer` feature so the Swift macos-shell build is unaffected. Re-exported from prelude.

CLI: `openwhisper transcribe <file>` now takes any supported file (hound dep dropped). JSON keeps text/confidence/duration_ms (CI smoke contract) and adds audio_ms/decode_ms/chunks.

UI: Transcribe sidebar route (transcribe-pane.tsx + use-file-transcription.ts), tauri-plugin-dialog 2.7.3 (Rust + JS pinned to same minor), dialog:allow-open capability, native webview drag-drop. Polls snapshot at 5 Hz only while busy.

Verified: 120 core unit tests, CLI smoke, 130 Playwright (8 new in transcribe.spec.ts), 22 vitest. Real files through CLI: opus, mp3, m4a, flac, 44.1k stereo wav, mp4/mov/webm video. Job state machine smoke (ignored test file_job_smoke) on a 2-min WhatsApp opus: decode 3.9 s, 5 chunks in 0.9 s.

Known: Opus decode is ~30x realtime (pure-Rust decoder), so a 1 h voice memo spends ~2 min in "Reading audio…" with an indeterminate bar. Ogg Vorbis untested with a real file (no local encoder). Whole file is held as 16 kHz f32 (~230 MB/h).

Pending before Done: manual smoke in the real Tauri app on Mac AND Windows (pick + drop + cancel + copy), see MANUAL SMOKE in tests/transcribe.spec.ts.
<!-- SECTION:NOTES:END -->
