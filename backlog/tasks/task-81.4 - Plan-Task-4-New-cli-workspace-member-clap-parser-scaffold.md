---
id: TASK-81.4
title: 'Plan Task 4: New cli/ workspace member + clap parser scaffold'
status: In Review
assignee: []
created_date: '2026-05-04 15:10'
updated_date: '2026-05-06'
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
- [x] #1 cli/ is a workspace member, builds via cargo build -p openwhisper-cli
- [x] #2 cargo run -p openwhisper-cli -- --help prints subcommand list
- [x] #3 Each subcommand --help succeeds (handlers unimplemented but parser shape correct)
- [x] #4 No new dep introduced into core/ or apps/tauri/src-tauri/
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Landed in commit `20bd03b`. cli/Cargo.toml introduces clap 4 (derive), anyhow, serde_json, hound and depends on openwhisper-core (path, default-features = false, features = ["recognizer"]). Bin name is `openwhisper`. Commands directory has one file per subcommand; each handler currently bails with a "lands in TASK-81.x" notice that the later tasks (5–8) replace. cli/build.rs (added in commit `c14f61e` alongside Tasks 7+8) emits Swift rpath link-args so dyld finds libswift_Concurrency.dylib at runtime — `cargo:rustc-link-arg` does not propagate cross-crate.
<!-- SECTION:NOTES:END -->
