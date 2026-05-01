---
name: openwhisper-task-lifecycle
description: Task close-out rule for OpenWhisper — code-complete + tests-green is NOT Done. READ before flipping a Backlog task (or claiming a feature is "done", "shipped", "complete") past In Review. Applies to TASK-N, TASK-N.M subtasks, PR descriptions, end-of-turn summaries, and follow-up scheduling pitches.
---

# Task lifecycle: code-complete is not Done

## The rule

**Done means the user has reviewed the change in the running app and accepted it.** Tests passing, types clean, and a green Playwright run are necessary but not sufficient. The user owns the QA gate; Claude does not.

**Why:** Type checks and unit/integration suites verify *code correctness*, not *feature correctness*. Tauri UI behavior, TCC permission flows, hotkey wiring, audio capture, focus handling, and visual layout all routinely pass tests while feeling wrong in the actual app. Closing a task before the user has driven the feature in `pnpm dev:tauri` (or a real release artifact) erases the QA loop and bakes regressions into "Done" history that the team later trusts.

## Backlog statuses

`backlog/config.yml` declares four statuses:

```yaml
statuses: ["To Do", "In Progress", "In Review", "Done"]
```

| Status | Meaning | Who flips it |
|---|---|---|
| To Do | Not started | n/a |
| In Progress | Active implementation | Claude (when work begins) |
| **In Review** | Code complete, tests green, awaiting user QA | **Claude (after work finishes)** |
| Done | User has reviewed in the running app and accepted | **User only** (or Claude with explicit "mark done" instruction) |

Subtasks follow the same lifecycle. A parent task can sit at In Review while its subtasks individually move through In Review → Done as the user signs off on each.

## How to apply

**When you finish implementing a task:**

1. Run the verification (`pnpm tsc --noEmit`, `pnpm test:ui`, etc.) and confirm green.
2. Set Backlog status to **In Review**, not Done. Append the commit ref + verification summary to `notes`:
   ```bash
   backlog task edit task-65.4 -s "In Review" --notes "Commit: f52a35f. 51/51 Playwright; tsc clean. Awaiting user QA in pnpm dev:tauri."
   ```
3. End the turn with what's pending QA — file paths or routes the user should check, anything you couldn't verify yourself, any deferred steps (e.g., "live-shell smoke deferred to user — Mac TCC re-grant required").
4. **Do not** call the work "done" / "shipped" / "complete" in the close-out summary. Use "in review" / "ready for QA" / "awaiting your review".

**When the user accepts:**

User signals acceptance with phrases like "looks good", "ship it", "merge", "mark done", or by approving a PR. Only then flip In Review → Done.

**When the user pushes back:**

User says "still broken", "the row isn't appearing", "settings is wrong" → status stays at In Review (or drops to In Progress if the fix is non-trivial). Do not flip back to Done until the next round of QA passes.

## Edge cases

- **Pure refactor with no user-visible change** (e.g., lift helpers into a shared module): code review still belongs to the user, but a working test suite is the bulk of the QA. Still default to In Review unless the user has set up a standing autonomy rule for refactors.
- **Plan documents / Backlog metadata** (writing a spec, opening a follow-up task, editing CLAUDE.md): not in scope for this rule. These are immediate artifacts; mark Done on completion.
- **Multi-machine work** (release: Mac DMG on one box, Windows MSI on another): each machine's leg goes In Review when its build is signed; the parent task closes only when both have been driven by the user end-to-end.

## Anti-patterns this skill prevents

- "All 7 subtasks done" while the dev shell hasn't been opened.
- Closing a task on the strength of a green Playwright run alone.
- Trailing "ready to ship" / "shipped" language in the end-of-turn summary that telegraphs Done before user QA.
- Proposing a `/schedule` follow-up agent that assumes the just-finished work is live (e.g., scheduling a flag cleanup before the flag has even been QA'd in the app).

## Related skills

- `openwhisper-iteration-budget` — when QA fails, don't blow past two attempts before stopping to research.
- `writing-backlog-plans` — plans land in To Do; this skill governs what happens after the implementation phase.
