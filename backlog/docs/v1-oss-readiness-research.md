---
id: doc-v1-oss-readiness-research
title: 'v1 OSS-readiness research — Tailscale, Payload, Handy, OpenWhispr'
date: '2026-05-04'
status: research
related:
  - milestones/m-1
---

# v1 OSS-readiness research

Research input for milestone **m-1**. Compares OpenWhisper's current scaffolding against four reference projects and produces a prioritized punch-list. **The findings here are evidence; the m-1 task plan is the action.**

## Reference projects inspected

- **Tailscale** (`tailscale/tailscale`) — large mature Go project, polished community surface
- **Payload CMS** (`payloadcms/payload`) — TypeScript monorepo, strong contributor onboarding
- **Handy** (`cjpais/Handy`) — closest analog: open-source local dictation app, Rust + Tauri
- **OpenWhispr** (`OpenWhispr/openwhispr`) — name collision; Electron + React 19 + TypeScript

## Per-project findings

### Tailscale (`tailscale/tailscale`)

**Root files present:** `README.md`, `LICENSE` (BSD-3-Clause), `CODE_OF_CONDUCT.md`, `SECURITY.md`, `PATENTS`, `CODEOWNERS`, `VERSION.txt`, `Makefile`.

**`.github/`:** `CONTRIBUTING.md` (DCO-required, `git commit -s`, no fixup commits, squash-and-force-push), `dependabot.yml` (weekly `github-actions`; `gomod` is intentionally disabled between releases — clever pattern), `licenses.tmpl`, custom `actions/go-cache/`, ISSUE_TEMPLATE (`bug_report.yml`, `feature_request.yml`, `config.yml`).

**Workflows (19 files):** `test.yml`, `vet.yml`, `golangci-lint.yml`, `govulncheck.yml`, `codeql-analysis.yml`, `checklocks.yml`, `installer.yml`, `docker-base.yml`, `docker-file-build.yml`, `flakehub-publish-tagged.yml`, `kubemanifests.yaml`, `natlab-integrationtest.yml`, `ssh-integrationtest.yml`, `pin-github-actions.yml`, `webclient.yml`, `update-flake.yml`, `update-webclient-prebuilt.yml`, `request-dataplane-review.yml`, `cigocacher.yml`. The PR-gate fan-out (lint + vet + vuln + tests + integration) is the standard to aim for.

**CLI structure:** `cmd/` directory has 30+ binaries; `cmd/tailscale` (CLI) and `cmd/tailscaled` (daemon) are the user-facing ones. **This is the model for our headless-CLI work** — same internal packages back both the CLI and the daemon.

**Release signaling:** 151 GitHub releases, `VERSION.txt` at root, build scripts (`build_dist.sh`) burn commit IDs into binaries. SECURITY.md is one paragraph + one email — minimalism works at their scale.

### Payload CMS (`payloadcms/payload`)

**Root files:** `README.md`, `LICENSE.md` (MIT), `CONTRIBUTING.md`, `SECURITY.md`, `ISSUE_GUIDE.md`, `CLAUDE.md`, `AGENTS.md`.

**`.github/`:** `CODEOWNERS`, `PULL_REQUEST_TEMPLATE.md` (conventional-commits enforced, asks "which branch should this target?" — relevant if OpenWhisper ever forks a maintenance branch), `dependabot.yml`, `ISSUE_TEMPLATE/config.yml` routes feature requests to GitHub Discussions and explicitly tells v3 users the v3 branch is in maintenance.

**Workflows:** `main.yml` (PR CI), `audit-dependencies.yml`, `pr-title.yml` (validates conventional-commits), `triage.yml`, `lock-issues.yml`, `stale.yml`, `publish-prerelease.yml`, `post-release.yml`, `post-release-templates.yml`, `notify-next-canary.yml`, `wait-until-package-version.sh`, `dispatch-event.yml`. **567 releases** managed via `semantic-release`.

**Take-aways:** The `pr-title.yml` (conventional-commits validator) and `config.yml` (route feature requests to Discussions, not Issues) are both cheap wins. The `semantic-release` setup is overkill for OpenWhisper's release cadence — stick with hand-curated CHANGELOG until the project earns it.

### Handy (`cjpais/Handy`) — closest analog

**Root files:** `README.md`, `BUILD.md`, `CONTRIBUTING.md`, `CONTRIBUTING_TRANSLATIONS.md`, `LICENSE` (MIT), `CLAUDE.md`, `AGENTS.md`, `CRUSH.md`, `flake.nix`.

**Notable absences:** `CODE_OF_CONDUCT.md`, `SECURITY.md`, `CHANGELOG.md`, `NOTICE`, `GOVERNANCE.md`, `MAINTAINERS.md`. Handy is a useful reminder that a successful Tauri dictation project shipped to ~13k stars without `SECURITY.md` — but OpenWhisper should not copy that gap. Handy's missing CoC has been a recurring complaint thread in their issues.

**`.github/`:** `FUNDING.yml` (multi-source: GitHub Sponsors + custom donate URL + Buy-Me-a-Coffee + Ko-Fi all in one file), `PULL_REQUEST_TEMPLATE.md` (the best PR template I saw — explicit about feature freeze, requires human-written description, AI-assistance disclosure checkbox), `ISSUE_TEMPLATE/{bug_report.md, config.yml}`. The `config.yml` is interesting: it routes hot topics (post-processing, hotkeys) to specific *pinned discussion threads* rather than letting them spawn new issues — a sharp triage pattern.

**Workflows (9 files):** `build.yml` (callable), `build-test.yml`, `pr-test-build.yml`, `main-build.yml`, `release.yml` (`workflow_dispatch` → draft release → 7-platform matrix including ARM macOS/Windows/Linux + ARM64), `code-quality.yml`, `nix-check.yml`, `playwright.yml`, `test.yml`. **`release.yml` is the closest reference for our eventual release-CI work.**

**CLI surface:** GUI-only Tauri binary; the "Handy CLI" mentioned in their README is a separate Python project, not a real headless surface. **OpenWhisper has the chance to do better here on day one.**

**Release notes:** Auto-generated via `generate_release_notes: true` plus a hand-written intro paragraph for major releases (e.g. v0.8.2's "New Model Alert!"). Lightweight, works.

**Positioning:** "the most forkable speech-to-text app, not the best" — useful narrative move; OpenWhisper has a parallel angle in "the open alternative to Superwhisper".

### OpenWhispr (`OpenWhispr/openwhispr`)

**What it is:** Electron + React 19 + TypeScript desktop app. Tagline: *"The open-source and free alternative to WisprFlow and Granola. Privacy-first voice-to-text dictation with AI agents, meeting transcription, and notes."* Uses whisper.cpp + sherpa-onnx + llama.cpp. **2.9k stars, 67 open issues, 47 PRs**, latest release v1.6.10 (April 2026). Repo created June 2025.

**Functional overlap with OpenWhisper:** ~80%. Both target local Parakeet/Whisper dictation with cross-platform desktop UI. OpenWhispr layers AI agents + meeting transcription + notes on top — broader scope. OpenWhisper is the leaner local-first dictation primitive.

**Stylization:** **OpenWhispr** — capital O, capital W, lowercase remainder, no dash, no space. Domain `openwhispr.com`, docs `docs.openwhispr.com`, GitHub `github.com/OpenWhispr/openwhispr`, sponsor `buymeacoffee.com/openwhispr`.

**Topics on the repo:** `ai, anthropic, cross-platform, gemini, groq, linux, macos, nvidia, open-source, openai, parakeet, speech-to-text, transcribe, whisper, windows`. OpenWhisper currently uses overlapping topics — that exacerbates name confusion in GitHub topic-search results.

**Their `SECURITY.md` is genuinely well-written** (private vuln reporting, 48 h SLA, 7-day fix target, scoped attack surface including audio-file RCE, IPC, supply-chain). **Steal the structure.**

**Naming-collision discussion:** None found in their issue tracker. The `OpenWhisper` (no `r`) name appears unclaimed there — meaning the rename window is open but won't stay that way once OpenWhisper has any public footprint.

**Namespace to avoid for clean rename:** GitHub org/repo `OpenWhisper`/`openwhisper`, domain `openwhisper.com`, npm `openwhisper`, binary `openwhisper`, social `@openwhisper`, brew cask `openwhisper`, winget `OpenWhisper.openwhisper`. The visual + phonetic distance between `OpenWhispr` and `OpenWhisper` is one letter — search engines and humans will conflate them indefinitely. **Rename ships before v1.**

## Punch-list (synthesized from the above)

### A. Must-have for v1 OSS launch

| # | File / scaffold | Reference best-in-class | Why for OpenWhisper |
|---|---|---|---|
| A1 | `.github/workflows/ci.yml` — PR-gate: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, `pnpm typecheck`, `pnpm test:ui` (Playwright) on macOS + Windows runners | Handy's `build-test.yml` + `pr-test-build.yml` matrix | Right now nothing automatically catches a regression on Windows when a Mac contributor opens a PR. The whole point of the FluidAudio/sherpa-onnx split is two-platform parity — CI must enforce it. |
| A2 | `SECURITY.md` | OpenWhispr's `SECURITY.md` is the strongest reference (private vuln reporting + 48 h SLA + scope list). Tailscale's is the minimalist alternative. | A dictation app holds a global keyboard hook + microphone permission + bundled native binaries (FluidAudioBridge, sherpa-onnx). That's a textbook attack surface. Without `SECURITY.md` enabling private vuln reporting, the only path is a public issue — wrong outcome. |
| A3 | `CODE_OF_CONDUCT.md` (Contributor Covenant 2.1) | Standard across mature projects | GitHub auto-recognizes it and surfaces a "Community Standards" green checkmark on the repo profile. Costs nothing, removes a recurring excuse new contributors use to disengage. |
| A4 | `.github/ISSUE_TEMPLATE/` with `bug_report.yml`, `feature_request.yml`, `config.yml` (`blank_issues_enabled: false` + Discussions link) | Handy's `bug_report.md` is the closest template — explicitly asks for App Version, OS, CPU, GPU, log dir. Tailscale uses YAML forms (validated fields). | OpenWhisper bugs are 90% platform-specific (TCC drift, ANE vs CPU, hardened-runtime flags, Windows audio enumeration). Free-text issues produce useless reports. Forced fields collapse triage time. |
| A5 | `CODEOWNERS` | Tailscale + Payload both have one | Even with one maintainer today, this auto-requests review and signals ownership. Future contributors then know who to ping for `core/` vs `apps/tauri/`. |
| A6 | README hygiene pass: badges + screenshot/GIF + install one-liners | Handy: `brew install --cask handy` and `winget install cjpais.Handy` are the headline | OpenWhisper's pitch is invisible on a repo page without a 5-second demo GIF. |
| A7 | `BUILD.md` (or `apps/tauri/DEVELOPMENT.md`) | Handy's `BUILD.md` is the reference — per-platform prereqs, exact commands, "from-source install" recipe | The TCC-rebuild gotcha + FluidAudio rpath build.rs setup + Backlog.md task model are all things a new contributor will trip over within 30 minutes. Document once. |

### B. Should-have within v1.x (next 6–12 weeks)

| # | File / scaffold | Reference | Why |
|---|---|---|---|
| B1 | `CHANGELOG.md` (Keep-A-Changelog, hand-curated) | OpenWhispr keeps one; Handy relies on `generate_release_notes: true` (loses editorial layer) | The `openwhisper-releases` skill already cuts versioned releases — missing piece is human-curated narrative. |
| B2 | `.github/workflows/release.yml` — tag-driven, builds DMG + MSI matrix, attaches to draft Release | Handy's `release.yml` is a near-perfect template: `workflow_dispatch` → matrix across `macos-26`, `macos-latest`, `ubuntu-22.04`, `windows-latest`, ARM Linux/Windows. Reuses a `build.yml` as a callable workflow. | Currently the `openwhisper-releases` skill splits the build across two physical machines manually. CI matrix removes that handover. |
| B3 | `.github/dependabot.yml` for `cargo`, `npm`, `github-actions` (weekly) | Tailscale runs `github-actions` weekly and disables `gomod` between releases | FluidAudio + sherpa-onnx + Tauri pinned versions are security-relevant. Manual tracking will slip. |
| B4 | `.github/workflows/codeql.yml` for the JS/TS surface (and `cargo audit` step in CI) | Tailscale `codeql-analysis.yml` + `govulncheck.yml`; Handy has `codeql.yml` | Automated complement to A2. |
| B5 | PR template upgrade: cross-platform-test matrix checkbox + `pnpm test:ui` checkbox + Backlog task ID | Handy's PR template is the gold standard (human-written description, AI-assistance disclosure) | Existing PR template doesn't enforce cross-platform-test discipline that platform-gotchas skill exists to protect. |
| B6 | `docs/contributing/architecture.md` — diagram of `core/` ↔ `apps/tauri/` ↔ recognizer trait | Payload has separate `docs/`; Handy explains inline in `CONTRIBUTING.md` | Externalize the `openwhisper-orchestration-in-rust` rule so PRs don't leak orchestration into the shell. |
| B7 | Headless CLI: `[[bin]]` target on the Rust core (or new `cli/` crate) | **Tailscale's `cmd/tailscale` (CLI) and `cmd/tailscaled` (daemon) split** — same internal packages back both | Unblocks: (a) automated CI smoke tests without windowed runners, (b) headless contributors on Linux who can hack the recognizer without touching Tauri, (c) reproducible bug reports. Promoted into A-tier per user direction (UI built on top of CLI/library API). |

### C. Nice-to-have

| # | File / scaffold | Reference | Why |
|---|---|---|---|
| C1 | `.github/FUNDING.yml` | Handy: GitHub Sponsors + donate URL + Buy-Me-a-Coffee + Ko-Fi in one file | Costs nothing once a sponsor link exists; reinforces "free by default". |
| C2 | `GOVERNANCE.md` + `MAINTAINERS.md` | Neither Tailscale nor Handy carry these — most projects defer until 3+ maintainers | Premature for a single-maintainer project. |
| C3 | Demo GIF + screenshots in `docs/assets/` | Handy README is text-heavy and arguably weaker for it; Payload uses a banner asset | A 5-second pill-HUD GIF is the single highest-leverage README asset. |
| C4 | `.github/workflows/lock-issues.yml` + `stale.yml` | Payload has both | Only matters once issue volume > 50 open. |
| C5 | Discussions enabled + `config.yml` linking from issue templates | Both Handy and Payload route feature requests to Discussions | Backlog.md is source of truth, but Discussions is right inbox for community feature requests. |

## Rename candidates

Hard constraints: ≤12 chars, pronounceable, doesn't collide with `OpenWhispr`/`openwhispr.com`/`@OpenWhispr` GitHub org or with OpenAI's `whisper`. Conveys local-first, free, dictation. No tortured backronyms.

| Name | Rationale | Watch for |
|---|---|---|
| **Murmur** | Quiet voice metaphor, 6 chars, easy domain potential. Distinct from "whisper" while staying in same metaphor family. | `murmurhash` is a hashing algorithm — minor namespace collision in search results. |
| ~~Mumble~~ | Already a known FOSS voice app (the Mumble VOIP client) | Skip — too well-known for a different domain. |
| **Quill** | Writing-by-voice metaphor, 5 chars, premium feel. | `quilljs.com` (rich-text editor) takes the obvious dotcom; `@quill` orgs likely taken. |
| **Cricket** | Local, ambient, evocative; chirpy/quick metaphor matches push-to-talk. | Long association with the sport in en-GB markets. |
| **Spoke** | Past-tense of "speak", 5 chars, "spoke-of-a-wheel" community vibe. | Common English word — SEO will be hard. |
| **Vox** | 3 chars, classical, clean. | Vox Media owns the brand association in en-US. |
| **Dictum** | Latin "thing said"; transcribes well as a verb (`dictum transcribe`). | Has a legal-docs connotation. |
| **Parley** | Speak/negotiate; pairs neatly with the bundled Parakeet model. | None major. |
| **Echograph** | "Echo" + "graph" (writing); 9 chars, evocative of "speech written down". | Slightly clinical. |
| **Tellur** | Made-up, short (6), pronounceable, suggests "tell" + a mineral suffix. | Tellurium the element overlaps in chemistry searches. |
| ~~Pillpad~~ | Riffs on existing pill-HUD identity | Skip — too specific to current UI; brittle if UI changes. |

**Top three to investigate for domain/handle availability:** **Murmur**, **Parley**, **Tellur**. Murmur wins on metaphor fit; Parley wins on Parakeet pun; Tellur wins on namespace freshness.
