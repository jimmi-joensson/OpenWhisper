---
name: openwhisper-headless-first
description: Architecture rule — every behavior ships through the public library API + the headless CLI before/while the UI consumes it. UI is the headful layer on top of headless surfaces, never the only place a feature exists. READ before opening a new `#[tauri::command]`, a React-only feature, a private helper inside `apps/tauri/`, or any pub fn in `core/`. Two binary gates inside — start gate ("does `cli/src/commands/` already have a placeholder for the parent task?") and finish gate ("does `grep -r TASK-N cli/` return anything?"). If the parent task has a CLI placeholder, the CLI surface is part of the deliverable; "code-complete + UI green" is not done. Pairs with `openwhisper-orchestration-in-rust` (where logic lives) and TASK-81 doctrine ("CLI feature surface = UI feature surface = library surface").
---

# Headless-first surface discipline

## The two binary gates

**Gate 1 — before writing the first `#[tauri::command]` for a feature.** Ask:

```
Does `cli/src/commands/` already have a placeholder for the parent
task? Grep: `grep -rn "TASK-N\b" cli/`.
```

If a placeholder exists, the CLI surface is part of the parent task's deliverable. Plan the CLI work into the same change set, not a follow-up. Examples of placeholder shapes that count:

- A `cli/src/commands/<name>.rs` whose handler returns `bail!("... TASK-N")`.
- An `eprintln!("... see TASK-N ...")` warn-and-skip in a CLI handler.
- A `default_X_reader()` / similar factory in `core/` that returns `None` with a doc comment naming TASK-N.
- A trait in `core::diagnostics` with `#[non_exhaustive]` empty struct types and TASK-N comments.

If no placeholder exists, this is a green-field feature — file the CLI surface explicitly during planning.

**Gate 2 — before flipping the parent task to In Review or opening a PR.** Run:

```
grep -rn "TASK-N\b" cli/ core/src/
```

Any match in `cli/src/commands/` or in a `default_*_reader`-style factory in `core/` is an unfilled deliverable. The parent task isn't done. Flip the gate; ship the CLI wiring; then re-flip.

These are not heuristics. They are checks you literally execute. The cost of the two greps is seconds. The cost of skipping them, on TASK-78, was building seven Tauri commands + a full UI surface before noticing `cli/src/commands/crash_dump.rs` existed at all.

## The rule

Every user-visible or developer-inspectable behavior ships through **three layers, in this order**:

1. **Public library API** — a `pub` item in `core/` (or another workspace member) with a doc-comment, re-exported from `core::prelude` when canonical.
2. **Headless CLI** — a subcommand under `cli/src/commands/` with both human-text and `--json` output, OR an extension to an existing subcommand. Reachable without the desktop shell.
3. **Headful UI** — Tauri shell consumes the same library function the CLI does. The UI does not own behavior the CLI cannot reach.

If a feature lands in only the UI, it doesn't exist for contributors, CI, or scripted use. If it lands in only the library, the parity invariant from TASK-81 (`cli/src/main.rs:5-7`) silently rots. Both failure modes have happened before — this rule blocks them.

## Why

- **Contributor onboarding.** Anyone with `cargo run -p openwhisper-cli -- memory` can repro a behavior without building Tauri, signing the bundle, or granting TCC. Lowering that floor is the difference between an OSS project and a vendor app.
- **CI smoke-ability.** UI behavior needs Playwright + a windowed runner; library behavior runs in `cargo test`. CLI behavior runs in shell. The CLI is the cheapest dependable smoke surface — TASK-81.9 already runs `cli transcribe` in CI.
- **Bug repro flatness.** "Run this CLI invocation and paste the JSON" is faster than "open the app, click here, screenshot the panel."
- **Architecture discipline.** Forcing every behavior through a `pub fn` keeps `apps/tauri/src-tauri/` from re-growing the orchestration that TASK-81 just lifted out.

## How to apply

**When you propose a new feature:**

1. Identify the load-bearing function or type. Place it in `core/` (per `openwhisper-orchestration-in-rust`).
2. Re-export it from `core::prelude` if it's a canonical type a consumer would name.
3. Plan a CLI surface. If the feature has *something to inspect or do* headlessly today, file or extend a `cli/src/commands/<name>.rs` in the same task. If there's genuinely nothing to do yet (e.g. a state machine with no registered instances), write down the "lights up when X" trigger in the task notes and gate the CLI work on that follow-up.
4. *Then* wire the UI in `apps/tauri/`.

**When you finish a task:**

Before flipping it to In Review, ask:

- Is the new behavior reachable from `cli/src/main.rs --help`? If not, why not?
- Is the canonical type re-exported from `core::prelude`? If not, why not?
- Does the Tauri command (if any) just call the library, or does it own logic the CLI can't reach?

If any answer is "no" without an explicit follow-up task, fix it before the In Review flip.

## When the CLI surface waits

CLI parity *can* defer when the underlying library surface has no concrete instance to operate on. Example: `model_lifecycle::ModelHandle<T>` shipped without a CLI command because no recognizer was wrapped yet — the CLI surface (per-model rows) lights up once `TASK-62.4` registers handles. The discipline still holds: file the follow-up explicitly, link the future trigger in the parent task, and don't ship a CLI subcommand that prints "no data" forever.

This is a real exception, not a loophole. If you reach for it, write the follow-up task ID into the implementation notes so the gap is visible in Backlog.

## Anti-patterns this skill prevents

- Adding a Tauri `#[tauri::command]` that wraps logic which exists nowhere else — the command becomes the de-facto API.
- Adding a React-only feature (toast, modal, side-effect) that does work the CLI can't trigger.
- A `pub` core function that's never re-exported from `prelude` — external consumers have to memorize module paths to find it.
- A "Diagnostics" UI panel that shows numbers no `cargo test` or `openwhisper memory` invocation can independently confirm.
- "We'll add the CLI later" without filing the follow-up — *later* never lands.

## Why this skill didn't catch TASK-78 the first pass

A real failure case worth naming, not buried:

In the original TASK-78 implementation pass, seven Tauri commands shipped
(`crashes_list`, `crashes_read`, `crashes_delete`, `crashes_delete_all`,
`crashes_mark_read`, `crashes_unread_count`, `crashes_open_folder`) plus a
full React inspector — without touching the existing CLI placeholder at
`cli/src/commands/crash_dump.rs`. The placeholder had explicit `TASK-78`
deferred markers (`bail!("... TASK-78")`, `eprintln!("see TASK-78 ...")`)
and a stub `default_crash_reader() -> None` in `core::diagnostics`. The
parity gap was caught only after the user asked "this work was also added
to the CLI right?"

Three stacked failures:

1. **Plan didn't name the CLI surface.** `backlog/docs/plans/doc-23` listed
   "Tauri commands" as a deliverable; CLI parity lived only in this skill,
   not in the plan. The plan was treated as authoritative scope.
2. **No pre-action gate.** The earlier skill description said *"READ
   before adding a Tauri command"* — informational, not a checkable
   binary question. Adding seven commands didn't trigger me to load the
   skill body. Gate 1 above (now part of this skill) is the fix.
3. **Placeholder code was invisible.** The strongest possible signal —
   `bail!("... TASK-78")` already in tree — was never seen because no
   grep ever ran outside `apps/tauri/` and `core/src/crashes/`. Gate 2
   above is the fix.

Lesson: the gates run for free. The plan + skill descriptions are
fallible. Use the gates anyway, every time.

## Related

- `openwhisper-orchestration-in-rust` — the *where logic lives* skill. This skill is the *how layers expose it* skill.
- `writing-backlog-plans` — the plan author's discipline. CLI surface should be an explicit deliverable on every plan that has a `cli/src/commands/` placeholder. Reviewer check today.
- TASK-81 (`backlog/docs/specs/doc-24 - Library-API-audit-and-headless-CLI-—-design.md`) — the audit that introduced the parity doctrine; this skill makes it durable.
- `cli/src/main.rs` header doc — single-source statement of the parity invariant.
- `core/src/prelude.rs` — the canonical re-export surface; new `pub` types belong here.
