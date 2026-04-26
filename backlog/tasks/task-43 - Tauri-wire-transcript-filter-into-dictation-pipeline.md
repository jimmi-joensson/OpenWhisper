---
id: TASK-43
title: 'Tauri: wire transcript filter into dictation pipeline'
status: Done
assignee: []
created_date: '2026-04-26 20:35'
labels:
  - tauri
  - transcript
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Tauri shell delivered raw recognizer output to injection. macOS shell already calls openwhisper_core::transcript::process before dictation_deliver_transcript (apps/macos/App/DictationService.swift:208). Tauri lib.rs spawn_recognizer now mirrors that — apps/tauri/src-tauri/src/lib.rs runs transcript::process(&res.text) before dictation::dictation_deliver_transcript so EN/DA fillers strip, comma-runs collapse, and language detect runs identically across shells. Same crate source of truth (core/src/transcript.rs); no per-shell drift.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Tauri spawn_recognizer pipes recognizer output through openwhisper_core::transcript::process before delivery
- [ ] #2 cargo check clean from apps/tauri/src-tauri
- [ ] #3 pnpm test:ui green from apps/tauri
- [ ] #4 Manual: dictate 'um hello uh world' via pnpm dev:tauri yields 'hello world'
<!-- AC:END -->
