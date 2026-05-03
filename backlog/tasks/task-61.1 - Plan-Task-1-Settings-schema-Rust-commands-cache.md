---
id: TASK-61.1
title: 'Plan Task 1: Settings schema + Rust commands + cache'
status: Done
assignee: []
created_date: '2026-04-30 22:18'
updated_date: '2026-05-03 10:19'
labels:
  - 61-impl
dependencies: []
parent_task_id: TASK-61
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 BehaviorSettings includes pause_audio_during_dictation: bool default true with serde(default)
- [ ] #2 behavior.rs exposes pause_audio_during_dictation() reader, set_pause_audio_cache() writer, and the two Tauri commands
- [ ] #3 behavior_set_pause_audio_during_dictation persists, updates cache, emits behavior_pause_audio_changed
- [ ] #4 Commands registered in generate_handler; cache hydrated in setup() from loaded settings
- [ ] #5 cargo check clean and BehaviorSettings round-trip test green
<!-- AC:END -->
