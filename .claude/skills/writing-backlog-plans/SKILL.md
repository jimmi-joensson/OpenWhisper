---
name: writing-backlog-plans
description: Use when writing an implementation plan for a project that uses Backlog.md (detect via `backlog/` dir or `backlog.config.yml` at repo root, or project CLAUDE.md references the `backlog` CLI). Produces both a design-doc markdown in backlog/docs/plans/ AND a Backlog.md subtask tree under the parent task. Backlog is the single source of truth: tasks own status/ACs/notes, attached docs (specs + plans) live in `backlog/docs/`. Adds Backlog enforcement: subtask-per-plan-task, status tracking, commit-ref appending, ID-mapping reviewer check.
---

# Writing Backlog Plans

Wrapper skill. Produce the usual plan markdown AND a Backlog.md subtask tree that mirrors the plan task-for-task. The markdown is the design artifact; Backlog is the live work-in-flight state.

**Announce at start:** "I'm using the writing-backlog-plans skill to create the implementation plan."

## When to use

Use when ALL of:

- Repo has `backlog/` directory OR `backlog.config.yml` / `backlog/config.yml` / `.backlog/config.yml` at root
- Project CLAUDE.md references the `backlog` CLI, or the user invokes `backlog task …` verbs
- You are writing a plan for a multi-step task

For projects without Backlog.md, hand-roll the plan markdown using the same shape (bite-sized tasks, verifiable outcome ACs, per-task verification steps) — there is no other planning skill installed.

## Prereqs

Verify before planning:

- `backlog --version` returns a version. If not, stop and ask the user to install Backlog.md.
- Parent task exists: `backlog task <id> --plain` returns the task body. If the parent doesn't exist, create it first with `backlog task create "<title>" -d "<why>" --ac "..."` — the plan attaches under that parent.
- The spec lives somewhere linkable (usually `backlog/docs/specs/…`). Record the spec path to pass to `--doc` when creating subtasks.

## Process

The plan markdown is owned by this skill — there is no separate plan-writing skill installed. Embedded rules below.

### Plan markdown rules

- **Create the file via the Backlog CLI, never `Write` it from scratch.** Backlog's UI and `/api/docs` endpoint require frontmatter (`id`, `title`, `type`, `created_date`); files written by hand without it silently fail to register and show as empty entries in the sidebar. Same rule for specs and decisions. Concretely:
  - Plan: `backlog doc create "<plan title>" -p plans -t plan` → creates `backlog/docs/plans/doc-N - <title>.md` with valid frontmatter. Edit the body afterward.
  - Spec: `backlog doc create "<spec title>" -p specs -t spec` → same, under `backlog/docs/specs/`.
  - Decision: `backlog decision create "<title>" -s accepted` (or `proposed` / `rejected` / `superseded`) → `backlog/decisions/decision-N - <title>.md`. Decision filenames MUST follow `decision-N - …` even if frontmatter is correct; the API ignores other patterns.
  - Existing date-prefixed plan/spec files (`YYYY-MM-DD-<topic>.md`) keep working as long as their frontmatter is intact — leave them alone, but use the CLI for any new file.
  See `references/backlog-cli-cheatsheet.md` § "Creating docs and decisions" for full flag detail.
- **One markdown file per parent task.** After CLI creates the file, top header includes `**Backlog parent:** TASK-<N>` and (if exists) `**Spec:** backlog/docs/specs/<spec-filename>.md`.
- **Tasks** are `### Task N:` headings, each ~one commit's worth of work. 2–6 verifiable outcomes per task. If a task has more, split it.
- **Steps** are concrete: name files (verify they exist or mark `(new)`), give code-shape snippets when ambiguous, but don't write the full implementation in the plan.
- **Outcome ACs** at the end of each task: observable end-states ("test X committed and red", "command Y emits Z"), NOT step commands ("run pytest").
- **Ordering + dependencies** stated explicitly. If tasks can run in parallel, say so. Cross-plan deps named to subtask granularity (e.g. "depends on TASK-62.2", not just "depends on TASK-62").
- **Verification per task** — every task names how it'll be verified (cargo test, manual smoke with concrete steps, Playwright spec).
- **No deferred design decisions** — anything load-bearing decided in the plan, not at execution time. Acceptable to defer minor knobs.

### Backlog enforcement

1. Confirm parent task ID. Add a "Backlog parent: TASK-<parent>" line near the plan's top header so the linkage is explicit.
2. For each plan task `Task N`, create the matching Backlog subtask (see next section).
3. Attach the spec to the parent: `backlog task edit <parent> --doc backlog/docs/specs/<spec>.md`.
4. Attach the plan markdown to the parent: `backlog task edit <parent> --doc backlog/docs/plans/<plan>.md`.
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

Agents executing the plan update Backlog as they go. The contract is:

- Start: `backlog task edit <subtask-id> -s "In Progress" -a @claude`
- Per commit: `backlog task edit <subtask-id> --append-notes $'<sha> <one-line summary>'`
- Per AC completed: `backlog task edit <subtask-id> --check-ac N`
- Done (implementation + reviews green): `backlog task edit <subtask-id> --final-summary "..." -s Done`

The writer's job is to set up the subtasks so the executor has somewhere to write these updates. The writer does NOT mark anything In Progress or Done.

## Plan ↔ Backlog mapping table

Canonical reference for what goes where:

| Source | Lives in | Updated by |
|---|---|---|
| Design rationale (problem, goal, non-goals, behavior model, trade-offs) | `backlog/docs/specs/YYYY-MM-DD-<topic>.md` | spec author (often the user via brainstorming) |
| Implementation plan (ordered tasks + steps + verification) | `backlog/docs/plans/YYYY-MM-DD-<topic>.md` | this skill |
| Per-task work-in-flight (status, owner, ACs, notes, commit refs) | Backlog subtasks `TASK-<parent>.<N>` | this skill (creates), executor (updates) |
| Cross-cutting follow-ups discovered mid-task | New top-level Backlog tasks (`TASK-<next>`) | this skill (escalate from executor) |

If a piece of information doesn't fit one of these rows, stop and ask — don't invent a fifth location.

## Plan reviewer addendum

After the plan + subtask tree are written, dispatch a reviewer agent (the project's general-purpose agent — there is no dedicated reviewer subagent installed). Pass it the standard plan-quality criteria PLUS the verbatim Backlog checks from `references/plan-reviewer-addendum.md`. Minimum the reviewer verifies:

- Backlog parent task exists and is referenced in the plan markdown
- One Backlog subtask per plan task, ID `TASK-<parent>.<N>` matches plan `### Task N`
- Subtask ACs lifted from plan task outcomes (not step commands)
- Subtask labels include `<parent>-impl`
- Subtask statuses reflect reality (don't approve "all To Do" if implementation commits already exist)

Paste the verbatim fragment from the addendum file into the reviewer's context.

## Worked example: TASK-41

Parent TASK-41 (skill layer split) had 10 plan tasks → 10 subtasks `TASK-41.1` through `TASK-41.10`, each with status, ACs, and commit refs in notes. Inspect the live pattern:

- Plan markdown: `backlog/docs/plans/2026-04-13-skill-layer-split.md`
- Subtask tree: `backlog task 41 --plain` (parent) and `backlog task list -l 41-impl --plain` (all children)

When uncertain about shape, read TASK-41's subtasks. The `--append-notes` log with commit SHAs is the canonical example of the status flow.

## Related skills

| Skill | Purpose |
|---|---|
| `writing-skills` | Authoring agent skill files themselves (different concern from authoring plans) |

Spec authoring is upstream of this skill — usually the user supplies the design context via brainstorming in chat, and this skill captures it as a `backlog/docs/specs/` doc before drafting the plan.

Plan execution is downstream — the executor (or the user) picks up subtasks one at a time and updates Backlog status as commits land.

## Single source of truth

Backlog is the source of truth. Tasks own status, ACs, notes, and commit refs. Specs and plans live as `--doc` attachments under `backlog/docs/{specs,plans}/`. **Do not create a sibling docs tree elsewhere in the repo for plan/spec content** — if a piece of design or implementation context isn't reachable from `backlog task <id> --plain`, it's effectively lost. This skill enforces that contract.
