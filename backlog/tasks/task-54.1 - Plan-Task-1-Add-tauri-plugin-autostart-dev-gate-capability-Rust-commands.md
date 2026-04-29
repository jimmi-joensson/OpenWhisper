---
id: TASK-54.1
title: 'Plan Task 1: Add tauri-plugin-autostart, dev gate, capability, Rust commands'
status: To Do
assignee: []
created_date: '2026-04-29 17:43'
labels:
  - 54-impl
dependencies: []
parent_task_id: TASK-54
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 tauri-plugin-autostart in Cargo.toml; @tauri-apps/plugin-autostart in package.json
- [ ] #2 Plugin registered with MacosLauncher::LaunchAgent and --autostarted arg, gated on #[cfg(not(debug_assertions))]
- [ ] #3 autostart.rs exports autostart_get / autostart_set / autostart_supported; all three in generate_handler!
- [ ] #4 Capability file lists autostart:default
- [ ] #5 autostart_set emits autostart_changed event with the new boolean payload on success
- [ ] #6 cargo check clean from apps/tauri/src-tauri/
<!-- AC:END -->
