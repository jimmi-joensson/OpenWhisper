---
id: doc-53
title: End-to-end checks implementation plan
type: other
created_date: '2026-11-11T11:49:00Z'
---


**Backlog parent:** TASK-103 (End-to-end checks of normal use across Modes, Ask, Vision and Agent)
**Spec:** none as a separate file. What the user asked for is in the task description and under "Design decisions" below.
**Stacks on:** `task-99-agent`.

## What exists today (verified by running, 2026-11-11)

- Each feature has its own eval set behind the CLI (`refine eval`, `modes` cases, `ask eval`, `vision eval`, `agent eval`), each judging one thing a model decides. None of them runs a feature the way a person uses it, from a spoken question to the answer, and none covers what happens when something is missing.
- `openwhisper ask run` and `openwhisper agent run` go through the same functions as the app. `agent eval` already starts a local fixture server and reads its log, and can stand in a synthetic image for the screen.
- The system voice can stand in for a microphone: `say -o file.aiff` and `openwhisper transcribe file.aiff` gave the English test question back word for word in 0.6 s. The Danish system voice was misheard on one word in the first try, so spoken Danish cases need sentences that are checked to transcribe.
- Nothing here can press a hotkey, click a permission prompt or judge how a panel looks.

## Design decisions

1. **Two halves.** A: scenarios a machine can run and judge, `openwhisper scenarios`. B: a walkthrough for a person in the Dev app, with the expected result per step.
2. **A scenario is a person's situation, not a unit:** settings as a person would have them, a question as they would say it, and a check on what they would see (the answer, a notice, a hint, a step, an error message), plus how long it took.
3. **A scratch config folder for every run.** The runner never reads or writes the real config folders, the real history or the real dictionary. It refuses to start when its folder is one of them.
4. **Only the local fixture as a connection.** No scenario lists or calls a real connection. Screens are synthetic images. Places, names and messages are made up.
5. **Groups** (`--only GROUP` or a name prefix):
   - *everyday*: weather with web search on and a place set (the answer has a forecast for that place); weather with web search off (no made-up forecast, and the hint that points to the setting); what day is it, which app am I in, what computer is this (answered from device context); the same with that context switched off (it says it cannot know); a Danish question answered in Danish; an answer with no long dashes.
   - *spoken*: the system voice speaks the question to a file, the recognizer transcribes it, the rule pass and the ask run on the transcript. English and Danish. macOS only (it needs `say`); skipped with a plain reason elsewhere.
   - *app context*: an app in front that has an enabled connection is answered through the connection (a tool call in the fixture's log, no capture); an app without one takes the screen (a capture, no tool call); a screen phrase with a mapped app does both; Agent off never calls a tool.
   - *modes*: a custom mode changes the text as its instructions say; an app default wins over the selected mode; Dictation leaves a clean sentence alone.
   - *failures*: the CLI is not there; the CLI says it is signed out (a stand-in script); the endpoint is unreachable; a timeout; cancel in the middle of an answer (the process is gone); no screen permission, a protected app in front, no display (stand-in platform layer): the ask goes on as text and says why. Each must end with a plain message, a non-zero state where it failed, and no hang (a time limit per scenario).
6. **Report.** One line per scenario (pass, fail, skipped with the reason, time), totals, and `--json`. A scenario that could not run because of an account limit is "stopped", not "failed", like the eval sets.
7. **Offline self-test.** `scenarios --offline` proves each check accepts a right outcome and rejects the mistakes it is there for, without a model.
8. **The walkthrough** lives in `docs/` and is written for a person who has not seen the code: what to do, what should happen, what to note down when it does not. It covers what a machine cannot: the hotkeys, the panel and Insert in a real app, permission prompts, a real password manager on screen during a screen ask, one real connection with drafts only, and a short list for Windows.
9. **Not in this plan.** Driving the real app window, a microphone, any real connection, performance benchmarks.

## Task 1: The scenario runner and the everyday, modes and failure groups (TASK-103.1)

Files: `cli/src/commands/scenarios/{mod,everyday,modes,failures,report}.rs` (new), `cli/src/commands/mod.rs`, `cli/src/main.rs`, `cli/tests/scenarios.rs` (new).

Outcomes:
- `openwhisper scenarios list` and `openwhisper scenarios run [--only X] [--json] [--offline]` exist.
- The runner refuses a real config folder (test).
- Offline self-test passes for every scenario in these groups.
- A live run's numbers are in the task notes.

## Task 2: The spoken and app context groups (TASK-103.2)

Files: `cli/src/commands/scenarios/{spoken,app_context}.rs` (new), reuse of the fixture and the stand-in screen from `core/src/agent`.

Outcomes:
- Spoken English and Danish scenarios pass on macOS and are skipped with a reason elsewhere.
- App with a connection: a tool call and no capture. App without: a capture and no tool call. Checked from the fixture's log and the capture count.

## Task 3: The walkthrough (TASK-103.3)

Files: `docs/assistant-walkthrough.md` (new).

Outcomes:
- Steps with expected results for Modes, Ask, Vision and Agent on macOS, and a short list for Windows.
- No real names, no screenshots.

## Task 4: Run and record (TASK-103.4)

Outcomes:
- The full live run, twice, with pass and time per scenario, is in the TASK-103 notes.
- Cargo tests and the Windows target check pass.
- TASK-103 is In Review with what a person still has to do (the walkthrough).

## Ordering

1, then 2 and 3 side by side (no shared files), then 4.
