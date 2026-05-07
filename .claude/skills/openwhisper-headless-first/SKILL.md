---
name: openwhisper-headless-first
description: Architecture rule — every behavior ships through the public library API + the headless CLI before/while the UI consumes it. UI is the headful layer on top of headless surfaces, never the only place a feature exists. READ before adding a Tauri command, a React-only feature, a private helper inside `apps/tauri/`, or any pub fn in `core/`. Pairs with `openwhisper-orchestration-in-rust` (where logic lives) and TASK-81 doctrine ("CLI feature surface = UI feature surface = library surface").
---

# Headless-first surface discipline

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

## Related

- `openwhisper-orchestration-in-rust` — the *where logic lives* skill. This skill is the *how layers expose it* skill.
- TASK-81 (`backlog/docs/specs/doc-24 - Library-API-audit-and-headless-CLI-—-design.md`) — the audit that introduced the parity doctrine; this skill makes it durable.
- `cli/src/main.rs` header doc — single-source statement of the parity invariant.
- `core/src/prelude.rs` — the canonical re-export surface; new `pub` types belong here.
