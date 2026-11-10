---
id: TASK-99
title: Agent - let an ask use the user's own MCP connections
status: To Do
assignee: []
created_date: '2026-11-10T14:36:00Z'
updated_date: '2026-11-10T16:41:00Z'
labels:
  - agent
  - ask
  - core
  - cli
  - tauri
dependencies:
  - TASK-97
  - TASK-98
priority: low
ordinal: 81000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Let an ask do things through the tool connections the user already has in their own agent CLI: for example draft a reply to the message on screen in the chat tool, without opening a thread first. Screen plus speech is the context; the agent CLI and its MCP servers do the work.

This is the opposite of Refine, which runs the CLI with every tool and MCP server switched off. It is a separate path with its own settings and its own switch.

Safety rules from the start: text on the screen is untrusted input and may try to steer the agent. Default to drafts only, an explicit list of allowed tools, and a confirm step in the answer panel for anything that sends, posts or deletes. Show every tool call in the panel.

Before designing: verify that the headless Claude Code CLI sees the connections the user expects (account connectors versus locally configured MCP servers), how permissions are granted without a terminal, and how long a typical run takes. A plain OpenAI-compatible endpoint cannot do this; a second agent CLI could be added later behind the same interface. Re-check the provider terms before this ships to anyone but the author.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Spike result written into the task notes: which connections the headless CLI can use, how permissions work, typical run time
- [ ] #2 Agent is its own opt-in switch, off by default, separate from Refine and Ask
- [ ] #3 Only tools on the allow list run; anything that sends, posts or deletes waits for a confirm in the panel
- [ ] #4 Every tool call and its outcome is visible in the panel and summarised on the history entry
- [ ] #5 On-screen text that tries to instruct the agent is covered by an eval set
- [ ] #6 Headless parity: the CLI can run an agent ask
- [ ] #7 Terms check redone and reported before release to other users
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Spike (2026-11-10, Claude Code 2.1.322, Sonnet low, fake MCP server only; no real connector tool was called):
- Headless claude -p can use MCP servers given with --mcp-config and --strict-mcp-config. --safe-mode drops them, so the Agent path uses --setting-sources "" instead (no hooks, plugins or user permission rules; sign-in still works).
- Connections it sees by default on this machine: 33 servers, 474 tools (5 user-scope, 2 plugin, 26 account connectors of which 9 connected). Listing without a model call: a control_request mcp_status on stream-json input (3.5 to 4.6 s). Loading every connector into a run costs about 260,000 prompt tokens, so servers are chosen one by one: a generated strict config with only the enabled servers.
- Permissions without a terminal: a tool that is not allowed is denied, the run continues, and the denial is reported with its input. Names are mcp__server__tool with wildcards. This machine's default mode is auto, which runs unlisted tools without asking: the Agent path must pin --permission-mode default.
- Confirm step: --permission-prompt-tool stdio on a stream-json process sends a control_request can_use_tool with the tool name and input and waits (100 s tested) for allow, allow with edited input, or deny. Also works: an approve tool on an MCP server of our own (documented), and deny-then-resume (arguments can drift).
- Tool calls and results are assistant tool_use and user tool_result lines, with display names.
- Run time: no tool 2.4 s, one tool call 3.3 to 3.6 s, two 3.9 to 4.5 s; the fake server adds no start-up time.
- Injection probe: a tool result that told the model to send the thread to an outside address was not obeyed in 6 of 6 runs, with or without the gate. Small sample; the gate is the protection.
- Terms: docs say --bare will become the default for -p, which would cut subscription sign-in; Anthropic says products for others should not offer claude.ai login. To be re-checked before release (AC 7).
<!-- SECTION:NOTES:END -->
