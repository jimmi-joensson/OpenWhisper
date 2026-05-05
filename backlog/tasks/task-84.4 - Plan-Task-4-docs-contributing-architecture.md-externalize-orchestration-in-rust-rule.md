---
id: TASK-84.4
title: >-
  Plan Task 4: docs/contributing/architecture.md (externalize
  orchestration-in-rust rule)
status: To Do
assignee: []
created_date: '2026-05-04 16:22'
updated_date: '2026-05-04 16:26'
labels:
  - 84-impl
milestone: m-1
dependencies: []
parent_task_id: TASK-84
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Author docs/contributing/architecture.md (~200 lines). Direct prose translation of .claude/skills/openwhisper-orchestration-in-rust/SKILL.md so non-agent contributors can read it. Includes: the rule, why, what-lives-where lists, ASCII diagram (reuse from TASK-81 spec), recognizer trait pointer, Tauri command thinness rule, when-you-find-drift guidance, footer linking skill as source of truth.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 docs/contributing/architecture.md committed
- [ ] #2 Body is prose translation of the orchestration-in-rust skill — same axioms, only framing differs
- [ ] #3 ASCII diagram renders identically in GitHub web view, IDE preview, and cat (no Mermaid/SVG)
- [ ] #4 What-lives-where coverage: state machine, transcript pipeline, recognizer trait, hotkey hook, NSPanel ops, tray menu, settings schema, fullscreen detection, TCC reset
- [ ] #5 Footer cites .claude/skills/openwhisper-orchestration-in-rust/SKILL.md as source of truth
- [ ] #6 Reviewer reads BOTH skill and architecture.md and confirms rules are identical
- [ ] #7 Recognizer trait section uses correct name: Recognizer (not SpeechRecognizer) at core/src/recognizer/mod.rs:55, with impls FluidAudioBridge (Mac) and OrtParakeet (Win)
<!-- AC:END -->
