---
id: TASK-61.7
title: Apply audio ducking to Audio settings mic-test preview
status: In Review
assignee: []
created_date: '2026-05-02'
updated_date: '2026-05-02'
labels:
  - 61-impl
dependencies: []
parent_task_id: TASK-61
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 `audio_preview_start` calls `pause_audio_for_recording` before opening the mic
- [x] #2 `audio_preview_stop` calls `resume_audio_after_recording` after closing the mic
- [x] #3 Both paths respect the existing `pause_audio_during_dictation` master toggle (no ducking when off)
- [x] #4 Preview→recording transition is idempotent: starting a recording while preview is active does NOT double-pause / overwrite the controller's `paused_sessions` state
<!-- AC:END -->

## Implementation Notes

### What shipped

Wired the existing `pause_audio_for_recording` / `resume_audio_after_recording` helpers into the preview path. The same SMTC pause/resume + BT switchback wait that fires on a real recording now fires on Audio settings → Test microphone:

- `apps/tauri/src-tauri/src/lib.rs::audio_preview_start` calls `pause_audio_for_recording()` before `audio::audio_preview_start()`.
- `apps/tauri/src-tauri/src/lib.rs::audio_preview_stop` calls `resume_audio_after_recording()` after `audio::audio_preview_stop()`.

### Idempotency guard for preview → recording transition

`pause_audio_for_recording()` now early-returns when `PAUSED_BY_US.load() == true`. Without this guard, the sequence "preview opens → music paused, PAUSED_BY_US=true → user hits hotkey → recording starts → audio_start_capture stops the preview internally without firing our preview_stop hook → second pause_now resets the controller's `paused_sessions` to a fresh enumeration, which finds nothing Playing (already paused), returns false, leaving an empty Vec. The eventual resume_audio_after_recording would then have nothing to resume. The guard makes the second pause a no-op so the original paused_sessions list survives until resume.

### Files
- `apps/tauri/src-tauri/src/lib.rs` (idempotency guard + preview wiring)

### Validation
- `cargo check` clean from `apps/tauri/src-tauri/` after the change.
- Manual smoke pending on user box: open Settings → Audio → Test microphone → music pauses → Stop test → music resumes after BT delay (if BT) or instantly (if wired).
