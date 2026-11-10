---
id: TASK-97
title: Ask - second hotkey, spoken question, answer in a panel
status: In Progress
assignee: []
created_date: '2026-11-10T14:36:00Z'
updated_date: '2026-11-10T17:38:00Z'
labels:
  - ask
  - core
  - cli
  - tauri
dependencies:
  - TASK-93
documentation:
  - backlog/docs/plans/doc-50 - Ask-implementation-plan.md
priority: medium
ordinal: 79000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
A second, rebindable hotkey for asking instead of dictating. Speech is transcribed locally as usual, sent to the configured model as a question, and the answer is shown in a small panel near the pill instead of being typed into the focused app. Plain dictation never needs a model to decide whether something was a question.

Default binding: the dictation key plus one modifier (same pattern as other dictation apps: Fn for dictation, Fn plus a key for the second mode). Check whether Fn can be bound on macOS.

Needs: the hotkey, an ask pipeline in core (separate from the Refiner trait: answers can be long and may stream), the answer panel (copy, insert at cursor, dismiss), and history entries marked as ask. This panel is also the surface the vision and agent tasks use.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A second rebindable hotkey starts an ask; the dictation hotkey behaves exactly as before
- [ ] #2 The answer appears in a panel and is never typed into the focused app unless the user chooses insert
- [ ] #3 The panel shows progress, errors and a cancel; closing it cancels the request
- [ ] #4 Works with both backends; opt-in with a plain statement of where the question is sent
- [ ] #5 Headless parity: openwhisper ask reaches the same core function
- [ ] #6 Latency measured per backend and written into the task notes
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Feasibility (2026-11-10, Claude Code 2.1.322, Sonnet low, run by a subagent and re-run by the orchestrator for streaming and image input):
- Hotkey: both shells hold two hard-coded slots (toggle, cancel). macOS takes modifier taps and chords, Windows chords only; chords match exactly, so Ctrl+Shift+Space and Ctrl+Space do not collide. Slots become a list in TASK-96.3; Ask adds one. The dictation key is tap-to-toggle, there is no hold mode. Fn cannot be bound on macOS today (not in the modifier table).
- Streaming: the lean Refine flags plus --include-partial-messages give stream_event lines; text arrives as content_block_delta / text_delta. The final result and usage lines are unchanged. Thinking deltas can precede text and are skipped.
- Latency text-only: cold first text 1.3 to 2.3 s, total 2.4 to 3.3 s; process spawned 3 s ahead: first text 0.5 to 0.7 s, total 1.4 to 1.5 s.
- Web search: --tools WebSearch --allowedTools WebSearch with the other lean flags. A question that searches took 16 to 20 s and the text arrives in one burst; one that does not, 0.9 s. This machine's CLI default permission mode is auto, so the Ask path pins --permission-mode default.
- One warm process can take the question later and a second user turn after the first result (context kept).
- OpenAI-compatible: streaming over the existing ureq client works against a fake SSE server (BufReader over the body); no real endpoint was available.

2026-11-10: plan doc-50, subtasks 97.1 to 97.6. Branch task-97-ask, stacked on task-96-modes.
<!-- SECTION:NOTES:END -->
