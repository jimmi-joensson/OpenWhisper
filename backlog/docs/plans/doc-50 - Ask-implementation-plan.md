---
id: doc-50
title: Ask - implementation plan
type: other
created_date: '2026-11-10T17:37:00Z'
---


**Backlog parent:** TASK-97 (Ask)
**Spec:** none as a separate file. The decisions the user took are in the task description and under "Design decisions" below.
**Stacks on:** `task-96-modes` (hotkey slots as a list, the `extra` line in history files).

## What exists today (verified in code and by running, 2026-11-10)

- One recording path: `dictation::run_toggle` starts capture, `run_stop_pipeline` transcribes, cleans, refines and hands the text to the injector. Nothing in core knows a second kind of recording.
- Refine's Claude backend (`core/src/refine/claude_cli.rs`) spawns the user's CLI with lean flags at record start, writes one stream-json user message at stop and reads one `result` line. The OpenAI-compatible backend posts one non-streaming request through `ureq`.
- Verified by running (TASK-97 notes): the same flags plus `--include-partial-messages` stream the answer as `text_delta` events; a process spawned ahead gives first text in 0.5 to 0.7 s; `--tools WebSearch --allowedTools WebSearch` enables web search only; the CLI's default permission mode on a machine can be `auto`.
- Windows: `main` and `pill`, both declared in `tauri.conf.json` and again in `tauri.dev.conf.json` (the dev file replaces the list). The pill is turned into a non-activating NSPanel at start-up; there is no third window yet.
- Hotkey slots are a list after TASK-96.3; adding a slot is a table entry plus an action.

## Design decisions

1. **Ask is its own switch, off by default** (`ask.enabled` in a new `ask` block of `settings.json`). The Ask pane says where the question is sent, with the same sentence Refine shows for the backend. With Ask off the ask hotkey is not installed and nothing is sent.
2. **Backend: the one Refine is set up with** (Claude Code CLI or the OpenAI-compatible endpoint, including the binary path, URL and key). Ask has its own model and effort (default: the Claude default, Sonnet at low effort), its own optional standing instructions and its own timeout (60 s; 120 s with web search).
3. **Its own hotkey slot**, `hotkeys.ask`: Right Option tap on macOS, Ctrl+Shift+Space on Windows, rebindable in Shortcuts. The dictation key behaves exactly as before.
4. **One recorder, two kinds of session.** The ask key starts the same capture and local transcription, marked as an ask. Whichever key is pressed while recording stops it; the recording keeps the kind it started with. At stop an ask goes through the rule pass and the dictionary, then to `ask::submit`: no Refine, no injector, no stats row, no dictation history entry.
5. **The answer is never typed by itself.** It only exists on the ask snapshot. "Insert" in the panel is a separate, explicit command that hands the finished answer to the injector.
6. **An ask never changes anything.** The model gets no tools, except web search when the user switched that on (`ask.web_search`, off by default; Claude backend only). The CLI is always started with `--permission-mode default`, so a machine whose default is `auto` cannot widen that.
7. **A separate pipeline from the Refiner trait** (`core/src/ask/`, feature `ask`): answers are long and stream. `AskBackend::ask` takes the question (text now, images in TASK-98), a sink for events (text, searching) and a cancel flag. Claude: one lean process per ask, spawned when the ask recording starts, killed on cancel. OpenAI-compatible: `stream: true`, read as server-sent events over the existing `ureq` client.
8. **One ask at a time**, held in core as `AskSnapshot` (phase `idle | listening | thinking | answering | done | error | cancelled`, question, answer so far, error, backend, model, timings, usage). A new ask replaces the one before it. The shell emits `ask_tick` when the snapshot changes.
9. **The answer panel** is a third window, `answer`, near the pill on the pill's screen: never takes focus (non-activating NSPanel on macOS, no-activate on Windows), so "insert" pastes into the app the user was in. It shows the state (listening, thinking, searching the web, an error), the question, the answer, and Copy, Insert, Dismiss, with Cancel while a request runs. Dismissing or closing it cancels the request. The cancel hotkey (Esc while recording) also cancels an ask that is still running; once the answer is complete Esc is left alone.
10. **History.** With history on, a finished ask is stored as an entry whose text is the question, with `kind=ask` on the `extra` line and the answer as a `> Answer:` quote under it. Cancelled and failed asks are not stored. The History screen marks such entries and shows the answer.
11. **Diagnostics.** Each ask is a row in `refine_runs` with source `ask` (time to first text in the detail, tokens, plan windows), so the plan usage estimate covers asks too.
12. **Headless parity.** `openwhisper ask run [TEXT]` (or stdin) streams the answer to stdout through the same `ask::answer` function; `ask status`, `ask config`, `ask eval`.
13. **Eval set** (a model decides what to answer): factual question, Danish in and Danish out, a misheard dictionary name, a request to do something (must say it can only answer, never claim it did), a question that would need the screen (must say it cannot see it, until TASK-98), an attempt in the question to make it reveal or drop its instructions, and length.
14. **Not in this plan.** Follow-up questions in the same conversation, reading the answer aloud, the screen (TASK-98), tools (TASK-99).

## Task 1: Core: ask module, settings, session kind, backends, history (TASK-97.1)

Files: `core/src/ask/{mod,prompt,claude_cli,openai_compat}.rs` (new), `core/src/settings/ask.rs` (new), `core/src/settings/mod.rs` (block; `HotkeySettings.ask`, `HotkeyTarget::Ask`, defaults), `core/src/dictation.rs` (session kind, `run_toggle_kind`, ask branch of the stop pipeline, cancel), `core/src/refine/claude_cli.rs` (share binary lookup, command set-up, usage parsing), `core/src/refine/openai_compat.rs` (share URL, key and error helpers), `core/src/history/{mod,tests}.rs` (`kind`, `answer`), `core/src/refine/history.rs` (source `ask`), `core/Cargo.toml`, `core/src/{lib,prelude}.rs`.

Outcomes:
- `AskSettings` defaults are off, no web search; `cleaned()` clamps the timeout; old settings files load.
- A recording started as an ask never reaches Refine, the injector or the stats writer (test with the stub backend and a recording injector); a dictation is unchanged (existing tests).
- Against a fake CLI the Claude backend streams text events in order, reports first-text time and usage, is killed on cancel and on timeout, and is started with `--permission-mode default`; web search adds exactly the two WebSearch flags.
- Against a local fake SSE server the OpenAI-compatible backend streams, stops on `[DONE]`, and reports HTTP errors plainly.
- A finished ask appends a history entry with `kind=ask` and the answer; old files parse as before; entries without the new fields render byte for byte as before.

Verification: `cargo test -p openwhisper-core --no-default-features --features tauri --lib`.

## Task 2: CLI: `openwhisper ask` and the eval set (TASK-97.2)

Files: `cli/src/commands/ask.rs` (new), `cli/src/commands/mod.rs`, `cli/src/main.rs`, `core/src/ask/eval.rs` (new).

Outcomes:
- `openwhisper ask run "question"` streams the answer and prints first-text and total time on stderr; `--json` gives one record; `--web-search`, `--backend`, `--model`, `--effort` override without saving.
- `ask status`, `ask config` and `ask eval` exist; `ask run` works with Ask switched off in settings (an explicit command is the opt-in), and says where the text goes.
- The eval set passes its offline self-test; a live run and the latency per backend are in the task notes.

Verification: `cargo test -p openwhisper-cli`; live runs against a scratch config folder.

## Task 3: Hotkey: the ask slot (TASK-97.3)

Files: `apps/tauri/src-tauri/src/hotkey/{mod,mac,windows}.rs`, `apps/tauri/src-tauri/src/settings/mod.rs`, `apps/tauri/src-tauri/src/lib.rs` (`do_ask_toggle`), `apps/tauri/src/lib/{use-global-hotkey,hotkey-format}.ts`, `apps/tauri/src/Settings.tsx`, both shims, `apps/tauri/tests/settings-window.spec.ts`.

Outcomes:
- The ask slot is installed only while Ask is on, with the platform default when the user has not chosen a key; it calls `run_toggle_kind(Ask)`.
- The cancel slot's gate also passes while an ask is running.
- Shortcuts shows an "Ask" row that can be recorded and reset (Playwright).

## Task 4: Shell: commands, ask tick, the answer window (TASK-97.4)

Files: `apps/tauri/src-tauri/src/ask.rs` (new), `apps/tauri/src-tauri/src/lib.rs`, `apps/tauri/src-tauri/tauri.conf.json`, `tauri.dev.conf.json`, `capabilities/default.json`, `apps/tauri/src/main.tsx`.

`ask_get`, `ask_set`, `ask_status`, `ask_start(text)`, `ask_cancel`, `ask_dismiss`, `ask_insert`, `ask_copy`; event `ask_tick`.

Outcomes:
- The `answer` window exists in both config files and the capability list, is hidden at start, is shown above the pill when an ask starts and hidden on dismiss, and never becomes the key window on macOS.
- `ask_insert` hands the finished answer to the injector and hides the panel; with no finished answer it does nothing.
- Dismissing while a request runs cancels it (the backend process is gone).

## Task 5: UI: answer panel, Ask pane, asks in history (TASK-97.5)

Files: `apps/tauri/src/AnswerPanel.tsx` + css (new), `apps/tauri/src/lib/{ask,use-ask}.ts` (new, TanStack Query for settings), `apps/tauri/src/components/{ask-pane,history-row,diagnostics-refine}.tsx`, `apps/tauri/src/lib/settings-panes.ts`, `apps/tauri/src/Settings.tsx`.

Controls: `Switch` for Ask and for web search, `Textarea` for instructions, `Select` for model and effort, `Button`s in the panel. The panel follows `openwhisper-animation-philosophy`.

Outcomes:
- The panel renders every phase from `ask_tick` (listening, thinking, searching, answering with text growing, done, error, cancelled) and its buttons call the matching commands.
- The Ask pane switches Ask and web search and shows where questions are sent.
- An ask in history is marked and shows its answer.

## Task 6: Tests and verification (TASK-97.6)

Files: `apps/tauri/tests/{ask,answer-panel}.spec.ts` (new), shims.

Outcomes:
- Core, CLI and shell cargo tests, the Windows target check and the Playwright suite pass.
- Latency per backend is in the TASK-97 notes (the OpenAI-compatible one only if an endpoint exists; otherwise said so).
- TASK-97 is In Review with the unverified items listed (pressing the key, the panel's look and placement, insert into a real app, Windows).

## Ordering

1, 2, 3, 4, 5, 6. Tasks 3 to 5 share `lib.rs`, `Settings.tsx` and the shims and run one after the other.
