---
id: doc-28
title: OSS community files — design
type: spec
created_date: '2026-05-04 16:00'
---

# OSS community files — design

**Backlog parent:** TASK-83
**Milestone:** m-1 — v1.0 public release readiness
**Source research:** `backlog/docs/v1-oss-readiness-research.md`

## Problem

OpenWhisper today has the legal minimum (`LICENSE`, `NOTICE`, `CONTRIBUTING.md`, `PULL_REQUEST_TEMPLATE.md`) but is missing the scaffolding contributors actually look for before opening a PR or filing an issue:

- **Security reporting path** — currently a contributor with a vuln has only the public issue tracker. For a dictation app holding a global keyboard hook + microphone permission + bundled native binaries, this is a real attack-surface failure mode (per the research findings on OpenWhispr's `SECURITY.md`).
- **Code of conduct** — GitHub's "Community Standards" check is red without one. Recurring excuse for contributors to disengage; cheap to fix.
- **Issue templates** — bug reports come in as free-text "X doesn't work" with none of the platform context the project's gotchas-skill catalogs as required for triage. First-touch users don't know the questions because they're not on the bug-reporter side of those gotchas.
- **CODEOWNERS** — no auto-review-request, no signal of ownership.
- **PR template depth** — current template covers the legal boilerplate but doesn't enforce the cross-platform-test discipline the platform-gotchas skill exists to protect (every gotcha in that skill was a regression that would have been caught by Mac smoke + Win smoke + Playwright).
- **Dependency hygiene** — no Dependabot. FluidAudio + sherpa-onnx + Tauri pins are security-relevant; manual tracking will slip.
- **Changelog discipline** — release notes today exist as `docs/release-N.M.0-handover.md` files, which are *internal handovers* not user-facing changelogs. New contributors and existing users have no canonical place to read "what changed in this version, and why".

## Goal

Close all seven gaps with static-markdown commits. Each file is small (50-300 lines). None of them require deep technical design — they require **good defaults baked in from project knowledge that already lives in skills**.

## Non-goals

- **GitHub Discussions content** — the issue-template `config.yml` *links* to Discussions and routes feature requests there, but enabling Discussions and seeding pinned threads is a manual GitHub UI step the maintainer does post-public-flip. Out of v1 scope.
- **CodeQL / cargo-audit / supply-chain scanning workflows** — separate concern from community files; deferred to v1.x.
- **FUNDING.yml + sponsor links** — premature for a single-maintainer pre-launch project; nice-to-have category in the research punch-list.
- **GOVERNANCE.md / MAINTAINERS.md** — premature until 3+ maintainers exist.
- **Translating Contributor Covenant** — English only for v1.
- **i18n issue templates** — English only for v1.

## Behavior model

```
Contributor lands on repo
        │
        ├──► Wants to file vuln       ──► SECURITY.md
        │                                  └─ Private GitHub Security Advisory
        │
        ├──► Wants to file bug        ──► .github/ISSUE_TEMPLATE/bug_report.yml
        │                                  ├─ Required: app version, OS+version,
        │                                  │             CPU arch, recognizer,
        │                                  │             reproduction, log excerpt
        │                                  └─ Pre-fills with platform-context
        │                                       fields the gotchas-skill needs
        │
        ├──► Wants to file feature    ──► .github/ISSUE_TEMPLATE/config.yml
        │                                  └─ Routes to GitHub Discussions
        │                                     (issues = bugs only)
        │
        ├──► Wants to open PR         ──► .github/PULL_REQUEST_TEMPLATE.md
        │                                  ├─ Existing legal boilerplate (preserved)
        │                                  ├─ Cross-platform-test checkbox
        │                                  ├─ Backlog task ID
        │                                  ├─ AI-assistance disclosure
        │                                  └─ Reviewer auto-pinged via CODEOWNERS
        │
        ├──► Wants to read history    ──► CHANGELOG.md
        │                                  └─ Keep-A-Changelog narrative
        │                                     (separate from internal handovers)
        │
        └──► Wonders about behavior   ──► CODE_OF_CONDUCT.md
                                            └─ Contributor Covenant 2.1
                                               + maintainer contact email

Background:
  Dependabot ─► .github/dependabot.yml ─► weekly bumps for github-actions,
                                          monthly for cargo + npm
```

## Trade-offs

| Choice | Alternative | Why this |
|---|---|---|
| `SECURITY.md` uses GitHub Security Advisory (private vuln reporting) | Plaintext email | GHSA gives the reporter a private channel without exposing an email address that leaks into the repo's git log forever. Reachable via the "Security" tab on every GitHub repo. Cost: zero. |
| YAML form templates for issues (`.yml`), not plain markdown (`.md`) | Markdown templates | YAML forms validate required fields — contributors can't submit a bug report without app version / OS / recognizer engine. Markdown templates are skippable; the project has eaten enough triage cost on free-text bug reports already. |
| Maintainer email is a project-owned address (e.g. `security@<project-domain>`) — **TBD** | Personal email (e.g. `joensson236@gmail.com`) | Personal email leaks into git history, harms long-term maintainability if ownership changes. **Open question for the maintainer**: pick a project address before TASK-83.1 (SECURITY.md) and TASK-83.2 (CoC) ship. The plan tasks *call out* the dependency; they don't pick the address. |
| `dependabot.yml` cadence: weekly github-actions, monthly cargo + npm | Daily everything (Tailscale-style for github-actions, but they go further) | Mid-cycle dep churn from cargo/npm wakes the maintainer with PRs that merge → CI runs → minutes burnt. Monthly batches them. github-actions is weekly because action pinning has a known security drift profile (post-`tj-actions/changed-files` 2025 incident). |
| Keep-A-Changelog format for `CHANGELOG.md` | semantic-release auto-generated, or none | Hand-curated narrative survives editorial intent ("v1.0 added Windows support"); auto-generated PR-list is noise. semantic-release is overkill at OpenWhisper's release cadence. |
| Reverse-walk recent releases (0.3.0 → 0.5.0) into the seed CHANGELOG | Start fresh from `Unreleased` | The handover docs at `docs/release-0.{3,4,5}.0-*.md` already record what shipped. Walking them back means a contributor reading `CHANGELOG.md` doesn't have to context-switch to release-handover docs to see what's in 0.4.0. |
| ISSUE_TEMPLATE `config.yml` blocks blank issues | Allow blank issues with a "use template" hint | Free-text bug reports are exactly the failure mode the new YAML templates exist to prevent. Allow-blank would make the templates optional in practice. |
| One PR template (current `PULL_REQUEST_TEMPLATE.md` upgraded), not multi-template | Per-purpose PR templates (`bug.md`, `feature.md`, `chore.md`) | Multi-template requires `?template=` query-string awareness from contributors and provides almost no extra signal at OpenWhisper's PR volume. One thoughtful template > three half-used ones. |
| CODEOWNERS at the path level, not the file level | Per-directory `OWNERS` files (Chromium / Kubernetes style) | Single-maintainer project; granular per-directory ownership is premature. Two CODEOWNERS lines (`core/` + `apps/tauri/`) is enough signal. |

## Risk register

- **Maintainer email TBD blocks SECURITY.md and CoC.** The project doesn't have a project-owned email address yet. If the maintainer doesn't pick one before these tasks execute, they'll either ship with a `TODO` placeholder (visible to contributors — bad look) or ship with a personal email (long-term wrong). Mitigation: TASK-83.1 and TASK-83.2 explicitly call out this dependency in their AC list; reviewer should block merge if either contains a placeholder.
- **`config.yml` routes to Discussions before Discussions is enabled.** Enabling GitHub Discussions is a maintainer UI step. If the issue templates ship referencing a Discussions URL that 404s, contributors hit a dead link. Mitigation: either enable Discussions before TASK-83.3 ships, or have the `config.yml` link be `contact_links` with a temporary "feature requests are routed via the maintainer's email until Discussions is enabled" line. Plan calls out which.
- **Dependabot opens PRs that fail CI** while TASK-82's CI workflow is still landing. Mitigation: ship Dependabot **after** TASK-82 ships and is green on `main`, so the first Dependabot PRs land into a working gate instead of a half-finished one.
- **CHANGELOG drift** — the seed CHANGELOG ships with 0.3.0/0.4.0/0.5.0 historical entries. Going forward, every PR that affects user-visible behavior must edit `Unreleased` section. Mitigation: PR template adds a "Did you update CHANGELOG?" checkbox.

## Cross-task dependencies

- **TASK-82 (CI workflow) ships before TASK-83.6 (dependabot.yml).** Dependabot PRs need a working CI gate to be useful.
- **TASK-83.5 (PR template upgrade)** preserves the existing legal boilerplate verbatim — see the existing template at `.github/PULL_REQUEST_TEMPLATE.md`. The upgrade adds checkboxes; it doesn't replace the legal section.
- **TASK-NEW-5 (rename) lands after this task** — every file authored here will need a rename sweep at the end. Plan includes a "rename touch-points" list per file so the rename task doesn't miss any of them.
