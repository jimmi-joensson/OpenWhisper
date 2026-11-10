---
id: TASK-99
title: Agent - let an ask use the user's own MCP connections
status: To Do
assignee: []
created_date: '2026-11-10T14:36:00Z'
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
