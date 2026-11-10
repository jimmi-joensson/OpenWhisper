---
id: TASK-96
title: Modes - named instruction sets with a default per app
status: In Review
assignee: []
created_date: '2026-11-10T14:36:00Z'
updated_date: '2026-11-10T21:32:00Z'
labels:
  - modes
  - refine
  - core
  - cli
  - tauri
dependencies:
  - TASK-93
  - TASK-94
documentation:
  - backlog/docs/plans/doc-49 - Modes-implementation-plan.md
priority: medium
ordinal: 78000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Refine has one instructions field today. Turn that into named modes the user can switch between and create: for example plain dictation, a mode that turns loose speech into a precise prompt for a coding agent, a professional tone, translate to English. Each mode is a name plus instructions, and optionally its own model and effort.

On top of manual switching, a mode can be the default for an app: when the focus context says the text goes into that app, its mode is used. This is the step beyond manual mode pickers.

Needs: storage for modes in settings, a switcher (pill, tray or hotkey; decide with the user), a per-app mapping UI fed by apps seen in history, and the active mode recorded on each history entry and Refine run.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Modes can be created, renamed, edited and deleted; one built-in mode keeps today's Refine behaviour
- [ ] #2 The active mode can be switched without opening Settings
- [ ] #3 An app can have a default mode; dictating into that app uses it, and the UI shows which mode was used and why
- [ ] #4 Refine off still means no model call, whatever the mode
- [x] #5 Headless parity: modes can be listed, set and used from the CLI
- [x] #6 The live eval set covers at least one non-default mode
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-11-10: plan doc-49 written, subtasks 96.1 to 96.6 created. Branch task-96-modes. Hotkey feasibility: slots are two hard-coded fields in both shells (verified in hotkey/mac.rs and hotkey/windows.rs); they become a list here so the cycle slot and the later Ask slot are additions.

2026-11-10 build finished on branch task-96-modes (last code commit 9333926).
Built: named modes for Refine (built-in Dictation plus user-made ones, each with instructions and optionally its own model and effort per backend), a default mode per app (apps from history and apps open now), switching on Home, in the tray menu and with an optional cycle hotkey (unbound until set), the mode recorded on the Refine outcome, the Refine run row and the history entry (second comment line, extra: mode=Name), openwhisper modes and refine run --mode / --app / --current-mode.
Tests run by the orchestrator: core lib 381, CLI 15 + 9 + 1, shell 43, unit 28, Playwright 241 (197 on main), Windows target checks clean.
Live eval (Claude CLI, Sonnet, low effort, cold): full set 23/24, 24/24, 24/24; the 8 custom-mode cases after the review fixes 8/8 and 8/8, latency median 1.5 to 1.8 s, max 2.4 s. Built-in mode median 1.5 to 1.6 s.
Independent review: no high-severity finding; the released parser was run against files written by the new code (no entry lost or merged). 11 findings fixed in 9333926.
Not verified, needs a person in the Dev app: pressing the cycle hotkey, the tray Mode submenu, the mode label on the pill, dictating into an app with a default mode (the focus read), anything on Windows at run time. ACs 2, 3 and 4 stay unchecked until then (4 is covered by stub tests only).
Known limits: a short answer can still pass the guard in a custom mode (the prompt holds that line; 0 misses in the live runs); a mode meant to expand five words or fewer falls back to what was said; the released 0.6.1 app shows the extra line as text; downgrading to 0.6.1 on the same config drops the modes block.
<!-- SECTION:NOTES:END -->
