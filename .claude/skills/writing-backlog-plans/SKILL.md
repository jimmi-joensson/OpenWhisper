---
name: writing-backlog-plans
description: Use when writing an implementation plan for a project that uses Backlog.md (detect via `backlog/` dir or `backlog.config.yml` at repo root, or project CLAUDE.md references the `backlog` CLI). Produces both a design-doc markdown in docs/superpowers/plans/ AND a Backlog.md subtask tree under the parent task. Composes superpowers:writing-plans for the markdown half; adds Backlog enforcement: subtask-per-plan-task, status tracking, commit-ref appending, ID-mapping reviewer check.
---

# Writing Backlog Plans

Wrapper skill. Produce the usual plan markdown AND a Backlog.md subtask tree that mirrors the plan task-for-task. The markdown is the design artifact; Backlog is the live work-in-flight state.

**Announce at start:** "I'm using the writing-backlog-plans skill to create the implementation plan."

## When to use

Use when ALL of:

- Repo has `backlog/` directory OR `backlog.config.yml` / `backlog/config.yml` / `.backlog/config.yml` at root
- Project CLAUDE.md references the `backlog` CLI, or the user invokes `backlog task …` verbs
- You are writing a plan for a multi-step task (same trigger as writing-plans)

Otherwise use plain `superpowers:writing-plans`.

## Prereqs

Verify before planning:

- `backlog --version` returns a version. If not, stop and ask the user to install Backlog.md.
- Parent task exists: `backlog task <id> --plain` returns the task body. If the parent doesn't exist, create it first with `backlog task create "<title>" -d "<why>" --ac "..."` — the plan attaches under that parent.
- The spec lives somewhere linkable (usually `docs/superpowers/specs/…`). Record the spec path to pass to `--doc` when creating subtasks.

## Process

**REQUIRED SUB-SKILL:** Use `superpowers:writing-plans` for the plan markdown itself — file structure, bite-sized tasks, TDD step shape, reviewer loop, execution handoff. Do not duplicate that here.

After the markdown plan is drafted (before the reviewer loop), run the Backlog enforcement steps:

1. Confirm parent task ID. Add a "Backlog parent: TASK-<parent>" line near the plan's top header so the linkage is explicit.
2. For each plan task `Task N`, create the matching Backlog subtask (see next section).
3. Attach the spec to the parent: `backlog task edit <parent> --doc docs/superpowers/specs/<spec>.md`.
4. Attach the plan markdown to the parent: `backlog task edit <parent> --doc docs/superpowers/plans/<plan>.md`.
5. Run the plan reviewer loop. Pass the reviewer the addendum in `references/plan-reviewer-addendum.md` so it also checks ID mapping.

For concrete command syntax, see `references/backlog-cli-cheatsheet.md`.

## Backlog subtask creation rule

At plan-write time — NOT deferred to execution — create one Backlog subtask per plan task. Match plan task numbers to subtask titles using the `Plan Task N:` prefix:

```bash
backlog task create "Plan Task N: <short title>" \
  -p <parent-id> \
  -l <parent-id>-impl \
  --ac "<step 1 outcome>" \
  --ac "<step 2 outcome>" \
  --ac "<step N outcome>"
```

Rules:

- `Plan Task N:` prefix maps 1:1 to the plan markdown's `### Task N:` heading. N starts at 1.
- `-p <parent>` nests the subtask. Backlog will auto-assign a hierarchical ID like `TASK-<parent>.<N>`.
- `-l <parent>-impl` label groups all implementation subtasks for filtering: `backlog task list -l <parent>-impl --plain`.
- ACs come from the plan task's verifiable outcomes — not the literal step commands. "Failing test written and confirmed red" is an AC; "run pytest" is not.
- Lift 2–6 ACs per subtask. If a plan task has >6 verifiable outcomes, it's probably too big for one task — split it in the plan first.

Do not create subtasks speculatively for follow-ups discovered later; those become new top-level tasks (see mapping table).

## Status flow during execution

Agents executing the plan (via `subagent-driven-development` or `executing-plans`) update Backlog as they go. The execution skills already own this, but the contract is:

- Start: `backlog task edit <subtask-id> -s "In Progress" -a @claude`
- Per commit: `backlog task edit <subtask-id> --append-notes $'<sha> <one-line summary>'`
- Per AC completed: `backlog task edit <subtask-id> --check-ac N`
- Done (implementation + reviews green): `backlog task edit <subtask-id> --final-summary "..." -s Done`

The writer's job is to set up the subtasks so the executor has somewhere to write these updates. The writer does NOT mark anything In Progress or Done.

## Plan ↔ Backlog mapping table

Canonical reference for what goes where:

| Source | Lives in | Updated by |
|---|---|---|
| Design rationale (rubrics, topology, content allocation) | `docs/superpowers/specs/YYYY-MM-DD-<topic>-design.md` | brainstorming / spec-review |
| Implementation plan (ordered tasks + steps + commands) | `docs/superpowers/plans/YYYY-MM-DD-<topic>.md` | writing-plans |
| Per-task work-in-flight (status, owner, ACs, notes, commit refs) | Backlog subtasks `TASK-<parent>.<N>` | writing-backlog-plans (this skill) |
| Cross-cutting follow-ups discovered mid-task | New top-level Backlog tasks (`TASK-<next>`) | this skill (escalate from executor) |

If a piece of information doesn't fit one of these rows, stop and ask — don't invent a fifth location.

## Plan reviewer addendum

When dispatching the plan-document-reviewer subagent (see writing-plans' review loop), append the Backlog checks from `references/plan-reviewer-addendum.md` to the reviewer prompt. Minimum the reviewer verifies:

- Backlog parent task exists and is referenced in the plan markdown
- One Backlog subtask per plan task, ID `TASK-<parent>.<N>` matches plan `### Task N`
- Subtask ACs lifted from plan task outcomes (not step commands)
- Subtask labels include `<parent>-impl`
- Subtask statuses reflect reality (don't approve "all To Do" if implementation commits already exist)

Paste the verbatim fragment from the addendum file into the reviewer's context.

## Worked example: TASK-41

Parent TASK-41 (skill layer split) had 10 plan tasks → 10 subtasks `TASK-41.1` through `TASK-41.10`, each with status, ACs, and commit refs in notes. Inspect the live pattern:

- Plan markdown: `docs/superpowers/plans/2026-04-13-skill-layer-split.md`
- Subtask tree: `backlog task 41 --plain` (parent) and `backlog task list -l 41-impl --plain` (all children)

When uncertain about shape, read TASK-41's subtasks. The `--append-notes` log with commit SHAs is the canonical example of the status flow.

## Related skills

| Skill | Purpose |
|---|---|
| `superpowers:writing-plans` | Universal plan-writing (markdown half); REQUIRED SUB-SKILL |
| `superpowers:brainstorming` | Upstream — produces the spec this skill plans against |
| `superpowers:subagent-driven-development` | Downstream — executes the plan; updates Backlog subtasks per task |
| `superpowers:executing-plans` | Downstream alternative — inline execution with checkpoints |

## Classification rubric

When something belongs in writing-plans vs here:

| Rule about… | Layer |
|---|---|
| Plan markdown structure, bite-sized tasks, TDD shape, file paths | writing-plans (universal) |
| Backlog CLI invocation, subtask creation, status flow, AC management | writing-backlog-plans |
| Spans both | writing-plans owns the universal; this skill adds Backlog enforcement on top |

If tempted to add harness-specific guidance (Backlog, Jira, Linear) to writing-plans, stop — write a sibling wrapper instead.
