---
id: TASK-97
title: Ask - second hotkey, spoken question, answer in a panel
status: In Review
assignee: []
created_date: '2026-11-10T14:36:00Z'
updated_date: '2026-11-11T08:14:00Z'
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
- [x] #5 Headless parity: openwhisper ask reaches the same core function
- [x] #6 Latency measured per backend and written into the task notes
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

2026-11-11 build finished on branch task-97-ask (last code commit a1ec0b3), stacked on task-96-modes.
Built: a second hotkey slot (Right Option tap on macOS, Ctrl+Shift+Space on Windows, installed only while Ask is on), the recorder with a session kind, core/src/ask (streaming Claude CLI and OpenAI-compatible backends, one ask at a time, cancel, web search as a setting), the answer window (non-activating panel above the pill: progress, errors, Cancel, Copy, Insert, Dismiss), the Ask settings pane with where the question is sent, asks in history (kind=ask, the answer as a quote) and in the Refine runs table, openwhisper ask run|status|config|eval.
Tests run by the orchestrator: core lib 499, CLI 19 + 11 + 9 + 1, shell 62 + 1, unit 35, Playwright 302, Windows target checks clean.
Latency (AC 6), Claude CLI 2.1.322, Sonnet, low effort: cold first text 1.1 to 1.8 s, whole answer 1.8 to 3.5 s; process started 3 s ahead (what the hotkey does) first text 0.6 to 1.2 s, whole answer 1.2 to 1.6 s; web search with a search 11 s, text in one burst; ask eval 12/12 three times, first text median 1.1 to 1.2 s. OpenAI-compatible backend: not measured, no endpoint. Its streaming, cancel and timeout were tested against a local fake server, and its HTTPS path against a real endpoint without a key (the 401 came back with its message).
Independent review: no high-severity finding. Confirmed by the reviewer: the CLI flags seen live in the process list (safe mode, tools off, strict MCP, no session persistence, permission mode default; with web search exactly WebSearch), the injector has two call sites (dictation delivery and the insert command, finished answers only), the released history parser run against files with ask entries (nothing lost or merged). 9 findings fixed in a1ec0b3.
Not verified, needs a person in the Dev app: pressing the ask key, the answer window showing and where, focus staying in the other app, clicks and text selection in a window that never takes focus, Insert landing in the other app, Esc cancelling a running ask, fullscreen Spaces, anything on Windows at run time (the process-tree kill behind a cmd shim is compile-checked only), a real OpenAI-compatible endpoint. ACs 1 to 4 stay unchecked until then.
Known: with web search on the CLI delivers the answer in one burst after the search, so the panel shows Searching and then the whole answer. An ask needs no Refine switch but uses Refine's backend settings.

2026-11-11, after the user tried the Dev build: three additions on task-97-ask.
1. Web search hint (c2e5888): with web search off on the Claude backend the model flags a question that needs current information with a first-line marker; core strips it and sets a suggestion; the panel shows a notice with a link to Settings, Ask; the CLI prints the same. Live eval: 6 of 6 questions that need the web flagged, 0 of 6 plain ones, twice.
2. Device context (6fa883f): date, time and time zone; the computer; the name of the app in front; language and region; each behind a switch, all on by default; an optional typed place; no location permission. The Ask pane previews the exact text; openwhisper ask context prints it. Not sent: clipboard, window titles, user name, installed apps, network name, IP.
3. No long dashes (6fa883f): refine.no_long_dashes, on by default; one rule line in the cleanup, mode and ask prompts, plus a normalising pass over model output.
Tests run by the orchestrator: core lib 590, CLI 45, shell 65 + 1, unit 40, Playwright 319, Windows target checks clean. Live: ask eval 32 of 32 twice; refine eval cold 29 and 31 of 32 with every miss a 5 s timeout typed as said (all 32 safe), warmed 32 of 32; the dash cases passed in every run. These three additions have not had an independent review yet; they are covered by the whole-stack review at the end.
Follow-ups to file as their own tasks (agreed with the user, not built): selected text in the app in front as context, follow-up questions, system Location Services, a Codex CLI backend.
<!-- SECTION:NOTES:END -->
