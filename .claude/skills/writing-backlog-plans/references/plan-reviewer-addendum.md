# Plan Reviewer Addendum — Backlog Checks

Addendum to the plan-review prompt the writing-backlog-plans skill dispatches. Paste the fragment below into the reviewer's context alongside the standard plan-review criteria. The reviewer is typically a general-purpose agent (no dedicated reviewer subagent ships with this skill).

## When to use

The plan-writer (writing-backlog-plans) appends this addendum to the reviewer dispatch. The reviewer runs the standard plan-quality review PLUS the Backlog-specific checks below.

## What the reviewer checks

Beyond the universal plan-review criteria:

1. **Backlog parent referenced in plan** — the plan markdown names the parent task ID (e.g. `Backlog parent: TASK-58`) near its top header.
2. **Parent exists** — `backlog task <parent> --plain` returns a task. If it doesn't, that's a blocker.
3. **One subtask per plan task** — for each `### Task N:` heading in the plan, a Backlog subtask exists with:
   - ID `TASK-<parent>.<N>` (hierarchical nesting via `-p`)
   - Title starting with `Plan Task N:`
   - Label `<parent>-impl` for grouping
4. **ACs lifted from plan outcomes, not commands** — subtask ACs express verifiable outcomes ("failing test committed and confirmed red") rather than step commands ("run pytest"). Commands live in the plan markdown; outcomes live in Backlog.
5. **Status reflects reality** — if commits referencing the plan already exist on the branch, the matching subtasks should NOT all be `To Do`. Mismatch between plan execution state and Backlog state is a blocker.
6. **Spec + plan attached to parent** — `backlog task <parent> --plain` shows the spec doc and the plan markdown under documentation refs.

## Verbatim fragment for reviewer prompt

Paste this block at the end of the plan-document-reviewer dispatch prompt, after the standard review criteria:

---

**Backlog.md enforcement (additional checks):**

This project uses Backlog.md. In addition to the plan markdown review, verify the Backlog subtask tree:

1. Run `backlog task <parent-id> --plain` and confirm the parent exists and references the plan + spec via `--doc` attachments.
2. Run `backlog task list -p <parent-id> --plain` to list direct children.
3. For every `### Task N:` heading in the plan markdown, verify a subtask exists with:
   - ID matching `TASK-<parent>.<N>`
   - Title starting with `Plan Task N:`
   - Label `<parent>-impl`
   - ACs that express verifiable outcomes from the plan task's steps (not literal shell commands)
4. Flag any of the following as ❌ Issues Found:
   - Missing subtask for any plan task
   - Subtask N present but numbering doesn't match plan `### Task N`
   - ACs copy-pasted step commands (e.g. "run pytest") instead of outcomes
   - Missing `<parent>-impl` label
   - Status on any subtask is `To Do` while commits for that task already exist on the branch (`git log --oneline` referencing the plan)
   - Parent task missing `--doc` attachment for spec or plan markdown

Report the Backlog checks in a dedicated section of your review output, separate from the plan-markdown critique, so the writer can address each layer independently.

---

## Notes for the writer

- The reviewer is advisory. If it flags ID drift but the drift is intentional (e.g. a plan task was split after subtasks were created), explain in your response and either (a) renumber subtasks to match, or (b) renumber plan task headings to match subtasks. Don't leave the mismatch.
- If the reviewer can't run `backlog` commands in its sandbox, have it verify the mapping from the plan markdown + a supplied `backlog task list -p <parent> --plain` output pasted into its context.
