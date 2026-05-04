---
id: doc-30
title: README + BUILD.md + architecture doc — design
type: spec
created_date: '2026-05-04 16:18'
---

# README + BUILD.md + architecture doc — design

**Backlog parent:** TASK-84
**Milestone:** m-1 — v1.0 public release readiness
**Source research:** `backlog/docs/v1-oss-readiness-research.md`

## Problem

The repo's three doc surfaces — landing (README), build (none today), architecture (none today) — are either out of date, missing, or buried inside agent-only skill files.

1. **README.md** is ~70 lines, has the right pitch ("local-first, free by default, BYO keys for cloud, open alternative to Superwhisper") but contains stale claims. The Status section says "Linux port to follow" (true) but "the retired SwiftUI shell lives at `archive/macos/`" (also true) — ok. The Install section still says "No pre-built binaries yet — the Tauri release pipeline is being rebuilt (see TASK-46)." TASK-46 is **Done**: signed/notarized Mac DMGs ship via the Releases page (per `INSTALL.md`). README hasn't been updated to match. There are no badges, no demo, no platform pills, no "what's it like" affordance for a cold visitor.
2. **No BUILD.md exists.** Contributor onboarding is split across `CONTRIBUTING.md` (skinny — covers PR legal boilerplate + a one-line dev-loop pointer) and `INSTALL.md` (end-user-only — covers Mac DMG install + permission grants). A new contributor cloning the repo doesn't have a single doc that walks them from clone → working build. The information lives in `.claude/skills/openwhisper-dev-workflow/SKILL.md`, but skills are agent-only — humans don't see them.
3. **No architecture doc.** The `openwhisper-orchestration-in-rust` skill captures the load-bearing rule "state machines + phase transitions + status strings live in Rust core, not the platform shell" — but only agents see it. A contributor proposing a feature has no public doc explaining why their `if phase === 'recording'` check in React belongs in `core/` instead. PRs leak orchestration into the shell because the rule isn't visible.

## Goal

Three deliverables that close those gaps without duplicating skill content:

1. **README rewrite** — sharp pitch, badges, accurate install/build/release links, demo placeholder. ~120-150 lines.
2. **BUILD.md** (new, repo root) — single canonical contributor onboarding doc. From clone to first commit. Pulls relevant content from the dev-workflow skill into prose a non-agent contributor can consume. ~250-300 lines.
3. **docs/contributing/architecture.md** (new) — externalize the orchestration-in-rust rule + recognizer trait + core/shell split. ~200 lines, with one ASCII diagram of the architecture.

## Non-goals

- **INSTALL.md retirement.** INSTALL.md is currently end-user Mac install + permission grants. It's correct for what it covers. The plan does NOT retire it. The plan adds Windows install once TASK-66 (signed Win MSI) ships — and that's a TASK-66 follow-up, not part of this milestone. INSTALL.md's role going forward: end-user "I want to install OpenWhisper". BUILD.md's role: contributor "I want to hack on OpenWhisper".
- **Demo GIF perfection.** Plan ships a placeholder if visual freeze isn't fully done. Pill HUD is mostly stable per the recent commits (TASK-70 pill scale, TASK-79 emitter stutter fix), but a 5-second clean recording requires a controlled environment we don't need to block on. Capture is Task 5; the README ships with a placeholder for now.
- **Multi-language docs.** English only for v1.
- **Sponsor/funding sections in README.** Premature — single-maintainer pre-launch.
- **Roadmap section.** Backlog.md is the source of truth for what's planned; pointing the README at it is enough.
- **Comparison tables vs Superwhisper / Wispr Flow / Handy / OpenWhispr.** Tempting but invites flame-war PRs and is brittle as those products evolve. Stick to the positive pitch ("local, free, BYO keys") without naming competitors except in the "Why" framing.
- **CONTRIBUTING.md rewrite.** Existing CONTRIBUTING.md is skinny but correct on the legal boilerplate. After BUILD.md ships, CONTRIBUTING.md gets a one-line "see BUILD.md for build / dev-loop" link. Not a full rewrite.

## Behavior model

```
Cold visitor lands on github.com/.../OpenWhisper
                │
                ▼
            README.md
        ┌───────────────┐
        │ • Pitch       │
        │ • Badges      │
        │ • Demo GIF    │──── (placeholder OK at v1.0)
        │ • Status      │
        │ • Install ───────► INSTALL.md   (end users)
        │ • Build  ────────► BUILD.md     (contributors, NEW)
        │ • Architecture──► docs/contributing/architecture.md  (NEW)
        │ • Backlog ───────► backlog/ + Backlog.md docs
        │ • License ──────► LICENSE + NOTICE
        └───────────────┘
```

Three readers, three docs, each with a single job:

| Reader | Doc | One-line job |
|---|---|---|
| User who wants to dictate | INSTALL.md | "Get the app on my Mac and grant permissions." |
| Contributor who wants to hack | BUILD.md | "Get a working build and run my first PR through CI." |
| Contributor who wants to make architectural decisions | docs/contributing/architecture.md | "Where should this new piece of logic live?" |

README is the sieve that routes each visitor to the right doc.

## Trade-offs

| Choice | Alternative | Why this |
|---|---|---|
| BUILD.md at repo root | `docs/BUILD.md` or `docs/contributing/build.md` | Repo-root files are what GitHub auto-surfaces in the "Code" tab. Contributors find a root `BUILD.md` without a sub-folder hunt. Handy and Tailscale both put it at root. |
| Externalize the orchestration-in-rust rule into a new doc | Just link the skill file from CONTRIBUTING.md | Skills are explicitly agent-instructions ("READ before...") — the prose shape is wrong for a human contributor. Translate once into a public doc; keep the skill as the agent shortcut. Drift risk is low because the rule itself is stable (it's an architectural axiom, not a moving target). |
| Demo GIF in README | Demo GIF in `docs/assets/` linked from README | Inline GIF is the highest-leverage element on a cold-visitor README — moving it into a sub-folder dilutes impact. Bandwidth/loading concern is moot at ~500 KB. |
| Badges row at the top of README | Badges at the bottom or none at all | Top badges signal "this is a real project" within ~200 ms of page load. Cost: 4-5 lines of markdown. Worth it. |
| ASCII diagram in architecture.md, not SVG | SVG (rendered cleanly on GitHub) or Mermaid | ASCII renders identically in every markdown viewer (GitHub web, IDE preview, terminal `cat`); SVG and Mermaid have rendering inconsistencies in agent-shown markdown contexts and require maintenance when the architecture shifts. Keep it boring. |
| Don't include a Linux section in README "Stack" | Add a placeholder | Linux is explicitly out of v1 scope. A "coming soon" placeholder invites contributors to file Linux-port issues we have to triage. Not now. |
| Stay on "OpenWhisper" through this task | Lower-case the eventual rename into the new docs | The rename happens as a final sweep (TASK-NEW-5) so the *whole* project flips together. Half-renamed docs in this task would make the rename's mass-replace harder, not easier. |

## Risk register

- **README inaccuracies are public-facing**. Stale claim ("no binaries yet") is bad enough; a fabricated feature would be worse. Mitigation: every claim in the rewrite must be true on `main` today; reviewer cross-checks each numbered claim against an actual file/PR/commit.
- **BUILD.md drift from skill content.** Once BUILD.md exists, the dev-workflow skill might evolve (e.g. new gotchas added) and BUILD.md falls behind. Mitigation: add a footer to BUILD.md stating "If you're an agent, read `.claude/skills/openwhisper-dev-workflow/SKILL.md` for the canonical version of these conventions." Humans get BUILD.md; agents get the skill; the skill is the source of truth.
- **architecture.md misrepresenting the rule.** Translating skill prose to public prose is where I'd most likely fabricate. Mitigation: reviewer reads BOTH the skill and the new architecture.md and confirms the rules are the same — only the framing changes.
- **Demo GIF capture risks an iteration loop.** Recording a 5-second clean dictation requires the right window/text-field/audio setup. Mitigation: ship the placeholder first; the capture lands in a separate commit (Task 5) so a slow GIF doesn't gate the rest of TASK-84.
- **CI-status badge points at a URL that doesn't exist until TASK-82 lands.** If TASK-84.2 ships the badges before TASK-82 ships the workflow, the badge renders as a broken-link image. Mitigation: badge URLs use the canonical `https://github.com/<org>/<repo>/actions/workflows/ci.yml/badge.svg` format which renders as "no status" before any runs exist (not as broken). Acceptable.

## Cross-task dependencies

- **TASK-82 (CI workflow)** ships before TASK-84.2 (badges) — or the CI badge has nothing to point at. Soft dep: 84.2 can ship first with a "no status yet" badge that resolves once 82 lands. Doc the soft dep in Task 2's body.
- **TASK-NEW-5 (rename)** runs *after* TASK-84. Every file authored here ("OpenWhisper" appears in the README pitch, in BUILD.md prereqs, in architecture.md context). Plan tasks compile a rename touch-points list per file for handoff.
- **TASK-83.5 (PR template upgrade)** ships independently — but its CHANGELOG checkbox depends on TASK-83.7 (CHANGELOG.md seed) existing. Not relevant to TASK-84 directly, just a related ordering note.
