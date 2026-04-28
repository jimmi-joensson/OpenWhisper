# Backlog CLI Cheatsheet

Concrete commands used by writing-backlog-plans. Pipe-friendly output with `--plain`.

## Contents
- [Creating subtasks](#creating-subtasks)
- [Inspecting & listing](#inspecting--listing)
- [Status flow](#status-flow)
- [Acceptance criteria](#acceptance-criteria)
- [Notes & final summary](#notes--final-summary)
- [Attaching specs & plans](#attaching-specs--plans)
- [Search](#search)
- [Multi-line input](#multi-line-input)
- [Gotchas](#gotchas)

## Creating subtasks

One subtask per plan task, at plan-write time:

```bash
backlog task create "Plan Task 1: Write failing test for parser" \
  -p 58 \
  -l 58-impl \
  --ac "Failing test committed" \
  --ac "Test fails for the expected reason"
```

Flags:

- `-p <parent>` — nest under parent; Backlog assigns `TASK-<parent>.<N>` automatically.
- `-l <label>` — repeat for multiple labels. `<parent>-impl` groups implementation subtasks.
- `--ac` — repeat for multiple ACs. **Not** comma-separated.
- `-d "..."` — optional description (why this task exists, one paragraph).
- `--priority high|medium|low` — optional.
- `-a @user` — usually omitted at creation; the executor assigns on pickup.

Do NOT set `-s "In Progress"` at creation. Leave status at default (`To Do`).

## Inspecting & listing

```bash
backlog task <id> --plain                       # single task, AI-readable
backlog task list --plain                       # all tasks
backlog task list -s "In Progress" --plain      # filter by status
backlog task list -l 58-impl --plain            # filter by label (all TASK-58 subtasks)
backlog task list -a @claude --plain            # filter by assignee
backlog task list -p 58 --plain                 # direct children of TASK-58
```

Always pass `--plain`. Without it, output is TUI-formatted and harder to parse.

## Status flow

Executor (not writer) runs these. Writer only sets up the subtasks.

```bash
# Start
backlog task edit 58.1 -s "In Progress" -a @claude

# Per commit
backlog task edit 58.1 --append-notes $'abc1234 Added parser skeleton\n'

# Per AC completed (1-indexed)
backlog task edit 58.1 --check-ac 1
backlog task edit 58.1 --check-ac 2 --check-ac 3    # multiple at once

# Done
backlog task edit 58.1 --final-summary "..." -s Done
```

## Acceptance criteria

```bash
backlog task edit 58.1 --ac "New criterion"                  # add
backlog task edit 58.1 --ac "A" --ac "B"                     # multiple
backlog task edit 58.1 --check-ac 1                          # check
backlog task edit 58.1 --check-ac 1 --check-ac 3             # multiple
backlog task edit 58.1 --uncheck-ac 2                        # uncheck
backlog task edit 58.1 --remove-ac 3                         # remove
backlog task edit 58.1 --check-ac 1 --uncheck-ac 2 --ac "C"  # mixed ops
```

Flag rules:

- Multiple `--ac` / `--check-ac` flags: ✓ supported.
- Comma-separated values (`--check-ac 1,2,3`): ✗ not supported.
- Ranges (`--check-ac 1-3`): ✗ not supported.

## Notes & final summary

```bash
# Implementation notes (progress log)
backlog task edit 58.1 --notes "..."                         # replace
backlog task edit 58.1 --append-notes $'line1\nline2'        # append

# Final summary (PR description)
backlog task edit 58.1 --final-summary "Outcome + key changes + tests"
backlog task edit 58.1 --append-final-summary "More detail"
backlog task edit 58.1 --clear-final-summary
```

Prefer `--append-notes` during execution so the log builds incrementally with commit SHAs.

## Attaching specs & plans

```bash
backlog task edit 58 --doc docs/superpowers/specs/2026-04-13-skill-split.md
backlog task edit 58 --doc docs/superpowers/plans/2026-04-13-skill-split.md
backlog task edit 58 --ref src/parser.ts --ref https://github.com/org/repo/issues/42
```

`--doc` for design artifacts and specs. `--ref` for source files and issue URLs.

## Search

```bash
backlog search "subtask" --plain                       # fuzzy across tasks/docs/decisions
backlog search "backlog" --type task --plain           # tasks only
backlog search "api" --status "In Progress" --plain    # with filters
```

Use for "did someone already track this?" before creating a follow-up task.

## Multi-line input

Shells do NOT expand `\n` inside double quotes. Use ANSI-C quoting (`$'...'`) or `printf`:

```bash
# Bash/Zsh — preferred
backlog task edit 58.1 --plan $'1. Write test\n2. Implement\n3. Commit'
backlog task edit 58.1 --append-notes $'abc1234 added parser\ndef5678 handled edge case'

# POSIX portable
backlog task edit 58.1 --notes "$(printf 'line1\nline2')"
```

`"...\n..."` passes a literal backslash+n — it does NOT become a newline. This is by design.

## Gotchas

- **Never edit task .md files directly.** Backlog rewrites them; your edits are lost or break metadata. Only `backlog task edit`.
- **`--ac` doesn't split on commas.** `--ac "A,B"` creates ONE criterion named "A,B". Use two `--ac` flags.
- **Subtask IDs are `<parent>.<N>`** — not dashed. Use `58.1`, not `58-1`.
- **Parent must exist before `-p <parent>` works.** Create the parent first if missing.
- **`--plain` everywhere for scripts.** TUI output is for humans and varies by terminal width.
- **Status strings are case-sensitive** and quoted: `"In Progress"`, `"To Do"`, `Done`.
- **Labels are flat strings.** `<parent>-impl` is just a convention — Backlog doesn't parse hierarchy from label text.
