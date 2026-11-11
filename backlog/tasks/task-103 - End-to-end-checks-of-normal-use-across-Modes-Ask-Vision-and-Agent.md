---
id: TASK-103
title: End-to-end checks of normal use across Modes, Ask, Vision and Agent
status: To Do
assignee: []
created_date: '2026-11-11T10:21:00Z'
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
- [ ] #1 A scenario runner reachable from the CLI runs every scenario in half A and reports pass or fail per scenario
- [ ] #2 Spoken end-to-end scenarios pass in English and Danish without a microphone
- [ ] #3 App-with-a-connection scenario shows a tool call and no capture; app-without shows the capture path
- [ ] #4 Every failure scenario ends with a plain message and no hang
- [ ] #5 A walkthrough document for the Dev app exists with expected results per step, macOS and Windows
- [ ] #6 Results of the run are written into this task
<!-- AC:END -->
