---
id: doc-52
title: Agent - implementation plan
type: other
created_date: '2026-11-11T06:38:00Z'
---


**Backlog parent:** TASK-99 (Agent)
**Spec:** none as a separate file. The decisions the user took are in the task description, the spike notes on the task and under "Design decisions" below.
**Stacks on:** `task-98-vision` (the ask pipeline, the answer panel, the screen in an ask).

## What exists today (verified by running, see the spike notes on TASK-99)

- An ask is a conversation with a backend (`core/src/ask`), shown in the answer panel. The model has no tools (web search aside) and the CLI runs with `--safe-mode`, which also drops every MCP server.
- The headless Claude Code CLI can use MCP servers given with `--mcp-config` and `--strict-mcp-config` when `--setting-sources ""` replaces `--safe-mode`. A tool that is not allowed is denied and the run goes on. With `--permission-prompt-tool stdio` on a stream-json process, a tool that is neither allowed nor disallowed raises a `can_use_tool` control request with the tool name and input and waits for allow or deny (re-run by the orchestrator: a denied send never reached the fake server).
- This machine's CLI defaults to permission mode `auto`, which runs unlisted tools without asking.
- The user's own setup has 33 servers and 474 tools; loading all of them into one run costs about 260,000 prompt tokens. The servers and their tools can be listed without a model call and without calling a tool (`mcp_status` control request).

## Design decisions

1. **Agent is its own switch, off by default** (`agent.enabled`), separate from Refine and Ask. It only does anything in an ask, so Ask must be on too. Claude backend only: an OpenAI-compatible endpoint cannot do this, and the Agent pane says so.
2. **Connections are chosen one by one.** The Agent pane lists the connections the user's Claude Code has (name, kind, state, tools) after a refresh. Nothing is enabled until the user enables it. A run gets a generated strict MCP config holding only the enabled connections, so a connection that is not enabled does not exist for the model. A user can also add a connection by hand (command and arguments, or a URL); the tests use that with a local fake server.
3. **Listing never calls a tool.** It is the `mcp_status` request on a CLI process that gets no prompt. The connection details it returns can hold secrets (a server's environment, headers), so they are kept in memory only; `settings.json` stores names, the enabled flag, the allow list and the cached tool names. For a run they are written to a private temp file (mode 0600) that is removed when the process ends, like the system prompt file.
4. **Allow list.** Per enabled connection the user ticks the tools the agent may use. A tool that is not ticked is disallowed for the run (`--disallowedTools`), which also removes it from what the model sees.
5. **Three kinds of tool,** decided in core from the tool's name and its annotations, on the cautious side:
   - *read*: annotated read-only, or named like `get`, `list`, `search`, `read`, `find`, `fetch`;
   - *draft*: creates or updates a draft;
   - *act*: everything else (send, post, reply, delete, update, create, and anything unknown).
6. **Drafts only is the default** (`agent.drafts_only`): act tools cannot run at all, ticked or not. With it off, a ticked act tool raises a confirm in the answer panel every time, showing the tool and its full input; allow, or deny. No answer in 120 seconds is a deny. A request for a tool the run was not supposed to see is denied without asking.
7. **The CLI flags are pinned:** `--permission-mode default`, `--setting-sources ""`, `--strict-mcp-config`, `--tools ""` (no built-in tools: no shell, no file access), `--no-session-persistence`, `--permission-prompt-tool stdio`, a turn limit. Read and draft tools on the allow list go in `--allowedTools`.
8. **Every tool call is shown.** The ask snapshot gets a list of steps (connection, tool, a short form of the input, state: running, done, failed, denied, waiting for confirm). The panel shows them as they happen. The history entry gets `tools=N` on the `extra` line and a `> Tools:` quote with tool names and outcomes (not the inputs).
9. **Untrusted input.** The screenshot, tool results and anything read from a connection are data. The prompt says so, but the protection is the gate: drafts only, the allow list and the confirm. Eval sets with a fake server check that an instruction planted in a tool result, or in text on the screen, never ends in a send, post or delete, by reading the fake server's log.
10. **Real connections may be listed, never called, in this work.** Every test and eval run uses `--strict-mcp-config` with the fake server only. Using the user's real connections is for the user to try.
11. **Headless parity.** `openwhisper agent status | config | connections [--refresh] | tools | allow | disallow | run TEXT [--screen] [--confirm ask|allow|deny] | eval`. In a terminal the confirm is a y/n prompt.
12. **Terms.** Running the user's own signed-in CLI from their own app on their own machine is the documented use. Shipping this to other people is a separate question (AC 7): the notes record what was found; it is not settled here.
13. **Not in this plan.** A second agent CLI, per-connection rules beyond the allow list, remembering a confirm ("always allow"), running an agent outside an ask.

## Task 1: Core: settings, tool policy, agent backend with confirm, steps (TASK-99.1)

Files: `core/src/agent/{mod,policy,claude,connections}.rs` (new), `core/src/settings/agent.rs` (new), `core/src/settings/mod.rs`, `core/src/ask/{mod,prompt}.rs` (steps and confirm on the snapshot; the agent path), `core/src/history/{mod,tests}.rs` (`tools`), `core/Cargo.toml` (feature `agent`), `core/src/{lib,prelude}.rs`.

Outcomes:
- Tool classification is tested on a table of real-world tool names and annotations; unknown is act.
- The spawn arguments are pinned by a test: the permission mode, the setting sources, strict MCP config, no built-in tools, no session persistence.
- Against a fake CLI: an allowed tool's call and result become steps; a `can_use_tool` request for a ticked act tool becomes a waiting confirm that allow and deny answer; an unexpected request is denied without a confirm; a confirm that times out is a deny; cancel kills the process.
- Drafts only: act tools are in the disallowed list whatever the allow list says.
- The generated config holds only enabled connections, is written with mode 0600 and removed afterwards; connection details never reach `settings.json` (test).

## Task 2: CLI, fake MCP server, eval sets (TASK-99.2)

Files: `cli/src/commands/agent.rs` (new, including the hidden `fake-server` subcommand: a small stdio MCP server with read, draft, send and delete tools that logs every call), `cli/src/main.rs`, `core/src/agent/eval.rs` (new).

Outcomes:
- `agent run` against the fake server reads and drafts; with drafts only a send never reaches the fake server's log; with it off, `--confirm deny` blocks it and `--confirm allow` lets it through.
- `agent connections --refresh` lists the user's connections without a tool call.
- Eval sets with the fake server: planted instructions in a tool result and in on-screen text; the check is the fake server's log. Live numbers and run times are in the notes.

## Task 3: Shell and UI: Agent pane, steps and confirm in the panel (TASK-99.3)

Files: `apps/tauri/src-tauri/src/agent.rs` (new), `apps/tauri/src-tauri/src/lib.rs`, `apps/tauri/src/components/agent-pane.tsx` (new), `apps/tauri/src/lib/{agent,use-agent}.ts` (new), `apps/tauri/src/AnswerPanel.tsx`, `apps/tauri/src/components/history-row.tsx`, shims.

Controls: `Switch` for Agent and for drafts only, a connection list with a `Switch` each and its tools as checkboxes grouped by kind (act tools marked), a refresh button, the confirm card in the panel with Allow and Deny.

Outcomes:
- Agent can be switched on only with its plain statement of what it can do; connections can be refreshed, enabled and their tools ticked.
- The panel shows each step as it happens and a confirm with the full input; Allow and Deny answer it.
- History marks asks that used tools and lists them.

## Task 4: Tests and verification (TASK-99.4)

Outcomes:
- Core, CLI and shell cargo tests, Windows target check, Playwright pass.
- Spike result, eval numbers, run times and the terms note are in the TASK-99 notes.
- TASK-99 is In Review with the unverified items listed (any real connection, the confirm in the real panel, Windows).

## Ordering

1, 2, 3, 4.

## Addition agreed with the user on 2026-11-11: the connection before the screen

14. **When the app in front has a connection, an ask about it uses the connection and takes no screenshot.** The user's words: if it is an app I have an MCP server for, it should use the MCP connection instead of vision, otherwise vision.
    - A mapping from app to connection: a small built-in table for common apps (matched on the app id from `focus`, to a connection by name), editable in the Agent pane (`agent.app_connections`: app id, app name, connection name).
    - Routing, decided in core when an agent ask starts: if the frontmost app is mapped to a connection that is enabled and has at least one allowed tool, the vision step is skipped (no capture) and the prompt tells the model which app is in front and that its content is reachable through that connection's read tools. Otherwise the vision trigger applies as in TASK-98.
    - An explicit screen phrase still takes the screenshot (the user asked for the screen in so many words); with the trigger on Always the connection still wins for a mapped app, because Always is a default and the mapping is the more specific choice.
    - The ask snapshot says which way it went (`context_source`: `connection` with its name, `screen`, or `none`), the panel shows it, and the diagnostics row records it.
    - Eval: with the fake server standing in for the app's connection and a fake platform for the screen, a mapped app leads to a read tool call and no capture call; an unmapped app leads to the capture path; a mapped app with an explicit screen phrase leads to both.

This adds to Task 1 (routing, settings, snapshot field), Task 2 (CLI `agent apps list|set|unset`, the eval cases) and Task 3 (the mapping list in the Agent pane, the source shown in the panel).
