---
id: TASK-98
title: Vision - the screen as context, only on request
status: To Do
assignee: []
created_date: '2026-11-10T14:36:00Z'
labels:
  - vision
  - ask
  - core
  - cli
  - tauri
dependencies:
  - TASK-97
priority: medium
ordinal: 80000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Let an ask include a screenshot so the user can refer to what is on screen ('look at my screen', 'answer this message'). Whole screen, not only the focused window. Never in dictation mode, never in the background.

When a screenshot is taken is a setting (decided 2026-11-10, wording open):
- Off.
- Phrases: a user-editable list, managed like dictionary terms, in English and Danish. Matched locally on the transcript before anything leaves the machine.
- Smart: phrases, or the model says the request needs the screen.
- Always: every ask includes the screen.

Every capture is visible on the pill and marked on the history entry. The image is held in memory only and dropped after the request.

Needs: screen capture on macOS (Screen Recording permission) and Windows, image input on the backends (check that the Claude CLI accepts an image the way we call it; the OpenAI-compatible API does), the trigger setting and phrase list, and a list of apps that must never be captured (password managers, banking).

Open points: which display on a multi-monitor setup (proposal: the one with the focused window); how Smart asks for the screen (a first text-only call that can answer 'need screen', or a tool the model calls).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 No screenshot is ever taken during dictation or without an ask in progress
- [ ] #2 Trigger setting with Off, Phrases, Smart and Always; default is Phrases; the phrase list is editable and ships with English and Danish defaults
- [ ] #3 Phrase matching runs locally on the transcript
- [ ] #4 Each capture is shown on the pill while it happens and recorded on the history entry
- [ ] #5 The image is never written to disk
- [ ] #6 Apps on the never-capture list block the capture and the answer says so
- [ ] #7 macOS permission flow is clear; Windows path verified on a real machine before leaving In Review
- [ ] #8 Headless parity: the CLI can run an ask with a screenshot
<!-- AC:END -->
