---
id: TASK-103
title: End-to-end checks of normal use across Modes, Ask, Vision and Agent
status: In Review
assignee: []
created_date: '2026-11-11T10:21:00Z'
updated_date: '2026-11-15T07:10:00Z'
labels:
  - testing
  - ask
  - vision
  - agent
  - modes
dependencies: []
priority: high
ordinal: 115000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The last step of the assistant roadmap (TASK-96 to TASK-99), asked for by the user on 2026-11-11: a full range of tests that a person would do when using the software. Two halves. A: scenario runs with real model calls that need no person: everyday asks (weather with web search on and a place set; weather with web search off shows the hint; what app am I in and what computer is this, answered from device context), spoken end to end without a microphone (the system voice speaks the question, the recognizer transcribes it, the ask runs; English and Danish), app context (an app in front with a connection is answered through the connection with no screenshot; an app without one takes the screenshot path; fake MCP server and synthetic images), modes (a custom mode, an app default winning), and failure cases (backend signed out or missing, timeout, cancel mid-answer, no screen permission, a protected app in front, display asleep): each must end with a plain message. B: a walkthrough for a person in the Dev app with the expected result per step: hotkeys, the panel and Insert, permission prompts, a real password manager on screen during a screen ask, one real connection with drafts only. macOS first, a separate short list for Windows.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A scenario runner reachable from the CLI runs every scenario in half A and reports pass or fail per scenario
- [x] #2 Spoken end-to-end scenarios pass in English and Danish without a microphone
- [x] #3 App-with-a-connection scenario shows a tool call and no capture; app-without shows the capture path
- [x] #4 Every failure scenario ends with a plain message and no hang
- [x] #5 A walkthrough document for the Dev app exists with expected results per step, macOS and Windows
- [x] #6 Results of the run are written into this task
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Plan: doc-53. Subtasks 103.1 to 103.4. Branch task-103-e2e, stacked on task-99-agent.

In review 2026-11-15, branch task-103-e2e (aea709c, d9ba49c, merges of task-99-agent).

Two product findings from the first runs, both fixed here:
1. Weather with web search on gave no forecast (6 of 6): search results carry no live data. An ask may now read live weather from api.open-meteo.com and geocoding-api.open-meteo.com when web search is on, and from nowhere else. Claude Code's fetch tool reads a list of hosts of its own without any rule, so the CLI is told to check with OpenWhisper before every fetch; verified live that a fetch of such a host is refused. The Ask pane says the place asked about is sent to open-meteo.com. This is a choice made on the user's behalf.
2. Claude Code itself tells the model today's date, the operating system, the shell, the model's name, its working folder and the email address of the signed-in account on every request (Ask, Agent, Refine), whatever is switched on in OpenWhisper. Nothing documented turns that off on a subscription login (two environment variables tried, no effect). The Ask and Refine panes and ask context now say so, and the two scenarios were split into what OpenWhisper sends (nothing) and what the answer may hold.

Still for a person: the walkthrough in docs/assistant-walkthrough.md. Nothing here pressed a hotkey, clicked a permission prompt, used a microphone or called a real connection.
<!-- SECTION:NOTES:END -->
