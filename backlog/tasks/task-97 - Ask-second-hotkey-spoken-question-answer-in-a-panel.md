---
id: TASK-97
title: Ask - second hotkey, spoken question, answer in a panel
status: To Do
assignee: []
created_date: '2026-11-10T14:36:00Z'
labels:
  - ask
  - core
  - cli
  - tauri
dependencies:
  - TASK-93
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
