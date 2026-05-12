---
name: openwhisper-branch-discipline
description: Branch-discipline rule for OpenWhisper — every code change ships from a feature branch, never directly to `main`. Parallel work uses `git worktree`, never two branches inside the main checkout. READ before any `git commit`, `git push`, or `git checkout`; before responding to "let's start on X" / "tackle Y" / "begin work on Z". Pairs with a PreToolUse hook (`.claude/hooks/branch-discipline.sh`) that blocks `git commit` / `git push` / `git merge` on `main` or `master` — the hook is the mechanical guarantee; this skill explains the why and the worktree pattern.
---

# Branch discipline — never commit to main

## The rule

**Every commit lands on a feature branch. `main` only receives merges, never direct commits.** Parallel work happens in **separate worktrees** on separate branches, not via stashing / branch-juggling inside the main checkout.

Two binary gates:

1. **Start gate** — before the first `Edit` / `Write` of a new chunk of work, check `git rev-parse --abbrev-ref HEAD`. If it returns `main` or `master`, stop and create a feature branch (`git checkout -b session-YYYY-MM-DD-<slug>` or task-named: `task-78.5-launch-toast`). Then proceed.
2. **Pre-commit gate** — before `git commit`, verify HEAD is not `main`/`master`. The `.claude/hooks/branch-discipline.sh` PreToolUse hook blocks this mechanically; the skill explains why so you don't try to work around the hook.

`git push` on a feature branch is fine. `gh pr create` opens the review surface; `gh pr merge` (or a manual GitHub click) is the only path commits reach `main`.

## Why

Concrete incident, 2026-05-12: a multi-commit session (TASK-81.2 / TASK-78.5 / TASK-81.3, six commits, ~700 LOC moved) landed straight on `main`. Locally tested fine. Two structural costs:

- **No PR-thread review surface.** GitHub PRs produce a single diff, file-by-file comment threads, and a check-list reviewers can work through. Six commits on `main` are reviewable only via `git show` per-sha — the user explicitly asked "how do I review this?" and the answer ended at "diff locally; cross your fingers for Windows."
- **Force-push is the only retroactive escape.** Once the work is on `main`, moving it to a branch requires `git push --force origin main` — destructive, rewrites history that's already published. The branch-first pattern makes this a non-question.

The benefit isn't theoretical: the very next step in that session was the user planning to test on a Windows box. If anything regresses on Windows, a feature branch's `git revert` is trivial; on `main` it still works but produces churn commits that read as confused project history.

The hook is the actual guarantee. The skill exists so future-Claude doesn't try to talk the user (or itself) into "just this once" on `main`. There is no "just this once" — it always feels justified in the moment, and the cost is structural.

## How to apply

**At the start of any chunk of work:**

```sh
git rev-parse --abbrev-ref HEAD
```

If `main` or `master`:

```sh
git checkout -b <slug>
```

Slug shape: `task-<N>[.<M>]-<short-name>` for backlog-task-driven work, `session-YYYY-MM-DD-<slug>` for multi-task sessions, `fix-<short-name>` for one-off bugfixes. The slug is for humans — make it greppable.

If already on a feature branch: continue. Don't switch branches mid-flow unless the new work is genuinely unrelated.

**At the end of a chunk:**

```sh
git push -u origin <branch>
gh pr create --title "..." --body "..."
```

The user is the maintainer. They merge the PR via GitHub UI (or `gh pr merge` after explicit ask).

## Parallel work — use worktrees, never branch-juggling

When two streams of work need to happen at the same time (e.g. one chunk waiting for CI / Windows feedback while a second chunk starts), do NOT switch branches inside the main checkout. Use a sibling worktree:

```sh
git worktree add ../OpenWhisper-task-78.5 -b task-78.5-launch-toast
cd ../OpenWhisper-task-78.5
# ...work in here independently...
```

Each worktree has its own working tree + index but shares the same `.git` data. Cleanup when the branch merges:

```sh
git worktree remove ../OpenWhisper-task-78.5
git branch -D task-78.5-launch-toast    # local cleanup if PR is merged
```

Worktrees beat branch-juggling for parallel work because:

- No `git stash` / `git stash pop` dance between branches.
- Background processes (`pnpm dev:tauri`, `cargo test --watch`) keep running per-worktree without rebuild churn from branch switches.
- Mistakes (committing to the wrong branch) are physically harder — you'd have to `cd` into the wrong directory first.

The Claude Code Agent tool has an `isolation: "worktree"` parameter that creates these automatically; prefer that when spawning agents on independent code-changing work.

## What counts as a "chunk of work"

- A backlog task or subtask → one branch.
- A multi-task session (the 2026-05-12 cleanup ran TASK-81.2 + TASK-78.5 + TASK-81.3 together) → one session-branch is fine; split into multiple branches only if the tasks could be reviewed separately and you actually want separate PRs.
- A typo fix in a doc → still a branch. It's three keystrokes (`git checkout -b fix-typo`), then `git push` + PR. Don't carve exceptions.

## Boundary — when NOT to branch

- Backlog status flips that document already-merged work (`status: In Review` → `status: Done` after a PR merge, no code change). These touch `backlog/tasks/*.md` only and are pure bookkeeping. A `Backlog: ...` commit straight to `main` is acceptable here because there's nothing to review — but only after the underlying code is already merged.
- Release-tagging commits per `openwhisper-releases`. Those have their own flow.

These exceptions are narrow. If you're unsure whether the change qualifies, branch.

## Related

- `.claude/hooks/branch-discipline.sh` — the mechanical enforcement; reads `tool_input.command` from the Bash PreToolUse event and blocks `git commit` / `git push` / `git merge` when HEAD is `main`/`master`. The hook explains itself in stderr and points back to this skill.
- `openwhisper-task-lifecycle` — what counts as "Done" past In Review.
- `openwhisper-backlog-first` — what to do before drafting plans.
- `openwhisper-iteration-budget` — when to stop retrying a misbehaving feature.

This skill is the open-side counterpart to `openwhisper-task-lifecycle` in the same way `openwhisper-backlog-first` is — discipline rules that bound when and how code changes ship.
