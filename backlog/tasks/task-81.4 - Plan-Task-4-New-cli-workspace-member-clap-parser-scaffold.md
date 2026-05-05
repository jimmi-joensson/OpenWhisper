---
id: TASK-81.4
title: 'Plan Task 4: New cli/ workspace member + clap parser scaffold'
status: To Do
assignee: []
created_date: '2026-05-04 15:10'
labels:
  - 81-impl
dependencies: []
parent_task_id: TASK-81
milestone: m-1
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add cli/ to root Cargo.toml workspace members. clap-derive enum with Transcribe, EnumerateDevices, RecognizerInfo, CrashDump subcommands. Empty handlers; --help shape works.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 cli/ is a workspace member, builds via cargo build -p openwhisper-cli
- [ ] #2 cargo run -p openwhisper-cli -- --help prints subcommand list
- [ ] #3 Each subcommand --help succeeds (handlers unimplemented but parser shape correct)
- [ ] #4 No new dep introduced into core/ or apps/tauri/src-tauri/
<!-- AC:END -->
