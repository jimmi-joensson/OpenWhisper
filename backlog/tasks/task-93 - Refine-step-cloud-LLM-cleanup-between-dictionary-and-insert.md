---
id: TASK-93
title: Refine step - cloud LLM cleanup between dictionary and insert
status: In Review
assignee: []
created_date: '2026-11-10T09:11:00Z'
updated_date: '2026-11-10T09:48:00Z'
labels:
  - refine
  - cloud
  - tauri
  - cli
dependencies: []
documentation:
  - backlog/docs/plans/doc-47 - Refine-step-implementation-plan.md
priority: high
ordinal: 64000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Opt-in Refine stage in core: after the rule pass + dictionary correction (core/src/transcript.rs process_tracked, called from dictation.rs run_stop_pipeline) and before dictation_deliver_transcript hands the text to the UI snapshot, the injector (cursor + clipboard) and stats. One interface, text in, text out.

Backends in v1: (1) Claude through the user's own installed and signed-in Claude Code CLI in headless mode (subscription, no API key in the app), (2) any OpenAI-compatible chat-completions endpoint (URL + model in settings.json, API key in the OS keychain).

Transcription stays local on Parakeet. No local LLM catalog, memory budget or downloads here (that is TASK-63, which can later plug in behind the same Refiner trait). Naming: doc-46 uses 'Selection-aware Refine' for a different, later feature (refine the marked text); this task owns the plain name 'Refine' for the post-transcription step.

Measured on 2026-11-10 (Claude Code 2.1.322, Haiku 5.5): cold CLI with default flags 6.6-9.4 s and it sometimes obeys the dictation; cold with lean flags 2.8-5.6 s; process spawned at record start and fed at stop 0.7-0.8 s. Hence: spawn one lean CLI process per dictation at record start, send the transcript at stop, kill on cancel. No conversation is shared between dictations.

Supersedes the Won't Do TASK-17 / TASK-45 direction.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Refine is off by default; with it off there is zero network or subprocess activity and the pipeline output is unchanged
- [ ] #2 With Refine on, text reaching the UI, the cursor and the clipboard is the refined text; the unrefined transcript stays available on the dictation snapshot and in the UI
- [ ] #3 Claude CLI backend works with the user's own signed-in CLI; missing CLI or signed-out state is detected and shown plainly in settings
- [ ] #4 OpenAI-compatible backend works with URL + model + key; key is stored in the OS keychain and never written to settings.json
- [x] #5 One composed prompt and one model call per dictation: guard rules + user instructions + dictionary terms, transcript passed as data
- [ ] #6 Timeout, backend error, missing backend or a suspicious output (empty or far longer than the input) all fall back to the unrefined transcript without blocking the paste
- [x] #7 Very short utterances skip Refine (no model call)
- [x] #8 Headless parity: openwhisper refine run/status/test reach the same core functions the app uses
- [x] #9 Tests cover prompt composition, fallback paths, both backends against fakes, and a live eval set (command-like dictation, question, Danish, mixed DA/EN, dictionary names)
- [x] #10 Settings pane: on/off, backend picker, instructions, test connection, and a plain statement of where the text is sent
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Shipped as one squashed commit through the task-93-refine pull request.
ACs 1-4 and 6 are implemented and covered by unit/Playwright tests but left unchecked until the feature has been dictated through in the running app (macOS and Windows). See TASK-93.7 notes for eval results, latency and the terms check.
<!-- SECTION:NOTES:END -->
