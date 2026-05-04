---
id: doc-29
title: OSS community files — implementation plan
type: plan
created_date: '2026-05-04 16:00'
---

# OSS community files — implementation plan

**Backlog parent:** TASK-83
**Spec:** `backlog/docs/specs/doc-28 - OSS-community-files-—-design.md`
**Milestone:** m-1 — v1.0 public release readiness

## Pre-work — maintainer decision

**Before any subtask executes**, the maintainer must:

1. **Pick a project-owned email address** for security disclosures and CoC enforcement contact (e.g. `security@<project-domain>`). Personal email is wrong long-term — leaks into git history and breaks if ownership changes. Until the rename ships (TASK-NEW-5), the address can be a generic project alias on a domain the maintainer controls. **Used by Tasks 1 and 2.**
2. **Decide whether to enable GitHub Discussions** before this task ships. If yes, the issue-template `config.yml` (Task 3) routes feature requests there directly. If not, `config.yml` lists a `contact_links` entry pointing at the maintainer email until Discussions is enabled.

These are not subtasks — they're decisions. The plan tasks call them out as blockers in their AC list.

## Ordering

```
1 (SECURITY.md)        ──┐
2 (CODE_OF_CONDUCT.md) ──┤
3 (ISSUE_TEMPLATE/)    ──┼──► Independent, parallel after pre-work
4 (CODEOWNERS)         ──┤
5 (PR template upgrade)──┘
                          ──► 6 (dependabot.yml) — after TASK-82 lands
                          ──► 7 (CHANGELOG.md seed) — independent
```

Tasks 1–5 and 7 are independent and can land in any order. Task 6 (Dependabot) lands after TASK-82's CI workflow is green on `main` so Dependabot PRs hit a real gate.

## Task 1: `SECURITY.md`

Author the file at repo root. Length: ~80 lines.

Structure (model after OpenWhispr's, per the research doc):

1. **Reporting a vulnerability** — link to GitHub Security Advisory (private reporting), instruct contributors not to file public issues for security bugs.
2. **Triage SLA** — acknowledge within 48 h, fix-target within 7 days for confirmed vulnerabilities.
3. **In-scope attack surface** — explicitly enumerate:
   - Audio-file RCE (a malicious WAV/MP3 fed to the recognizer triggering memory corruption in FluidAudio / sherpa-onnx / ort)
   - IPC abuse (Tauri commands invoked from a malicious WebView context, e.g. via XSS in a future cloud-provider integration)
   - Supply-chain compromise (FluidAudio bridge tampering, Parakeet model substitution)
   - Global keyboard hook abuse (the LL hook on Windows / CGEventTap on Mac being weaponized to leak keystrokes)
   - Microphone stream interception (the audio capture pipeline being snooped by another process)
4. **Out-of-scope** — denial-of-service against the user's own machine; bugs requiring local admin / sudo access; reports against archived branches.
5. **Acknowledgments** — section reserved for future "thanks to security researcher X" credits.

Maintainer email at the top under "Contact" (TBD per pre-work item 1).

**Outcomes:**
- `SECURITY.md` committed at repo root.
- File opens with a "Reporting a Vulnerability" section that names GitHub Security Advisories as the private reporting channel.
- 48h triage SLA + 7-day fix target stated explicitly.
- 5 in-scope attack surfaces enumerated by name (audio-file RCE, IPC abuse, supply-chain, keyboard hook, mic stream).
- Maintainer contact email present and project-owned (not personal). **Reviewer blocks merge if the email is a placeholder or a personal address.**

**Verification:** Visual diff review. Reviewer confirms the email is project-owned. Reviewer confirms GitHub renders the file correctly under the "Security" tab (this happens automatically when `SECURITY.md` lives at repo root).

**Rename touch-points** (for TASK-NEW-5): every "OpenWhisper" mention in the file body (target: 3-4 mentions); the project URL if any; the email address if it includes the project name.

## Task 2: `CODE_OF_CONDUCT.md`

Author the file at repo root. Length: ~140 lines.

- Use **Contributor Covenant 2.1 verbatim** from `https://www.contributor-covenant.org/version/2/1/code_of_conduct.txt`. Don't paraphrase — the unmodified text has legal+social legibility no rewrite preserves.
- Substitute the maintainer contact email in the "Enforcement" section (the placeholder is `[INSERT CONTACT METHOD]`).
- Substitute the project name in the title only (`# Code of Conduct` → keep as-is is also fine).

Don't customize the body. Contributor Covenant has been adopted by enough projects that any deviation gets noticed and questioned.

**Outcomes:**
- `CODE_OF_CONDUCT.md` committed at repo root.
- Body is verbatim Contributor Covenant 2.1 (verifiable via diff against upstream).
- "Enforcement" section names the maintainer's project-owned email (not personal, not a placeholder).
- GitHub recognizes the file and the "Community Standards" check goes green on the repo profile.

**Verification:** `diff` against upstream `code_of_conduct.txt` — only difference should be the contact-email substitution. Visual confirmation that GitHub picks up the file (Insights → Community Standards).

**Rename touch-points:** project name in the title (if substituted); contact email if it contains the project name.

## Task 3: `.github/ISSUE_TEMPLATE/{bug_report.yml, feature_request.yml, config.yml}`

Three files, all YAML form templates.

### `bug_report.yml`

Required fields (the YAML form will refuse to submit without these):

- `App version` — text input. Helper: "Find this under About in Settings, or run `OpenWhisper --version` from CLI once available (TASK-81.4)."
- `OS + version` — dropdown: macOS 14, macOS 15, macOS 16, Windows 10, Windows 11, Other. Plus a free-text `OS build` field for fine-grained version (e.g. "Win11 26200" — relevant to the BT gotcha).
- `CPU architecture` — dropdown: Apple Silicon (arm64), Intel x64 (Mac), x64 (Windows), arm64 (Windows), Other.
- `Recognizer engine` — dropdown: FluidAudio + Parakeet CoreML (Mac default), sherpa-onnx + Parakeet ONNX (Windows default), Don't know.
- `Audio output device` — text input (e.g. "AirPods Pro 2", "USB DAC", "Built-in speakers"). Surfaces the BT-render-kind context the BT mono-tail gotcha needs (`media_control/windows.rs::is_default_render_bluetooth` branches on this; tuning differs between BT-Classic, LE Audio, and wired).
- `Was OpenWhisper's own window focused when the bug occurred?` — radio: Yes / No / N/A. Critical for any "hotkey doesn't fire" or "Esc doesn't cancel" report — the WebView2-bypasses-WH_KEYBOARD_LL gotcha branches on this.
- `Reproduction steps` — textarea, required.
- `Expected vs actual behavior` — textarea, required.
- `Verbose log excerpt` — textarea, optional. Helper points at where logs go on each platform.

Optional fields:

- Screenshot/screen-recording attachment (just text encouragement; the GitHub form supports drag-and-drop natively).
- Has the bug appeared before this version? (yes/no/unknown).

Labels auto-applied: `bug`, `triage`.

### `feature_request.yml`

Lightweight — feature requests get routed to Discussions via `config.yml`, but a template exists for users who skip the routing.

Fields:

- `Use case` — what are you trying to do? Required.
- `Proposed approach` — what would you like the app to do? Optional.
- `Have you checked the project principles doc?` — checkbox. Helper links to `.claude/skills/openwhisper-project-principles/SKILL.md` so users see the design-direction-fit constraint before filing.

Labels auto-applied: `enhancement`, `triage`.

### `config.yml`

```yaml
blank_issues_enabled: false
contact_links:
  - name: Feature request / discussion
    url: https://github.com/<org>/<repo>/discussions/new?category=ideas
    about: Discussions is the right place for feature ideas. Use the bug report template for actual bugs.
  - name: Security vulnerability
    url: https://github.com/<org>/<repo>/security/advisories/new
    about: Please use private security advisory for vulnerabilities. See SECURITY.md.
```

If Discussions is not enabled at ship time, replace the first `contact_links` entry with a maintainer-email link. The plan's pre-work decision (item 2) determines which form ships.

**Outcomes:**
- `.github/ISSUE_TEMPLATE/bug_report.yml` committed; opening "New Issue" on the repo presents the form, and submitting without an app version + OS + recognizer is blocked by GitHub's form validation.
- `.github/ISSUE_TEMPLATE/feature_request.yml` committed.
- `.github/ISSUE_TEMPLATE/config.yml` committed; `blank_issues_enabled: false` is set; either a Discussions link or a maintainer-email contact link is present.
- Auto-labels (`bug`, `triage`, `enhancement`) defined in the YAML so triage doesn't have to manually label every new issue.

**Verification:** Open the repo on GitHub; click "New Issue"; confirm the two templates appear and the "Open a blank issue" link is gone. File a test issue from each template; confirm fields validate and labels apply. (Test issues can be deleted afterward.)

**Rename touch-points:** `<org>/<repo>` URL placeholders in `config.yml`; recognizer engine dropdown labels if "Parakeet" is renamed (it isn't — that's NVIDIA's name).

## Task 4: `CODEOWNERS`

Tiny file at `.github/CODEOWNERS`. Single maintainer for v1.

```
# CODEOWNERS — auto-request review on PRs touching matching paths
# https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/about-code-owners

# Default: maintainer reviews everything
*                       @<maintainer-github-handle>

# Explicit ownership signals for areas with concentrated platform knowledge
core/                   @<maintainer-github-handle>
apps/tauri/             @<maintainer-github-handle>
.github/                @<maintainer-github-handle>
backlog/                @<maintainer-github-handle>
```

The plan parameterizes on `<maintainer-github-handle>` (e.g. `@jimmi-joensson` per `apps/tauri/package.json` `homepage` / `repository` / `bugs.url` fields — the canonical record of the GH handle in the repo today).

**Outcomes:**
- `CODEOWNERS` committed at `.github/CODEOWNERS`.
- Opening a PR from a non-maintainer fork auto-requests review from the maintainer's GitHub handle.
- Branch protection (TASK-82.5) honors CODEOWNERS for required reviewers if the maintainer enables that toggle.

**Verification:** Open a PR (any change); confirm the maintainer auto-appears as a requested reviewer. Confirm GitHub Settings → Code security → CODEOWNERS shows no parse errors.

**Rename touch-points:** `<maintainer-github-handle>` if the maintainer's GH handle changes (likely no); the path globs (`core/`, `apps/tauri/`) if the rename moves directories.

## Task 5: `.github/PULL_REQUEST_TEMPLATE.md` upgrade

Edit the existing file. **Preserve the legal boilerplate verbatim** — the current "Legal" section at lines 7-19 is load-bearing for contribution rights.

**Layout discipline:** the new sections (Backlog task ID, Cross-platform smoke, AI-disclosure, CHANGELOG checkbox) all sit **above** the existing `---` horizontal-rule divider. The divider stays the divider between contributor-facing form and legal block. Don't strip it; don't refactor the legal section into its own H2 above the divider.

New shape:

```markdown
<!-- Describe your change. Reference any related backlog task (e.g. TASK-42). -->



## Backlog task

<!-- TASK-N or "no task" -->

## Cross-platform smoke

<!-- Tick every box that applies, or write "N/A — change does not affect that path" -->

- [ ] Built and exercised the change on macOS (run `pnpm dev:tauri`; verify the affected feature works)
- [ ] Built and exercised the change on Windows (run `pnpm dev:tauri`; verify the affected feature works)
- [ ] Ran `pnpm test:ui` from `apps/tauri/`; Playwright suite passes locally
- [ ] Ran `cargo test --workspace --exclude bench-sherpa --no-default-features --features tauri`; tests pass

## AI-assistance disclosure

<!-- Optional. If you used AI tools to draft significant portions of this PR (more than line-level autocomplete), say which tools and roughly which areas. Helps reviewers calibrate. -->



## CHANGELOG

<!-- If your change is user-visible, did you update CHANGELOG.md's Unreleased section? -->

- [ ] Updated CHANGELOG.md (or N/A — no user-visible change)

---

### Legal

I retain all rights, title, and interest in and to my contributions. By
opening this pull request and keeping this boilerplate intact, I confirm
that the OpenWhisper project (currently maintained by Jimmi Joensson, and
its successors and assigns) may use, modify, copy, distribute, sublicense,
and re-license my contributions under the project's choice of terms.

I represent that each contribution is my original creation, or that I
have the right to submit it under these terms (for example, my employer
has waived such rights in writing, or the contribution is in the public
domain).
```

**Outcomes:**
- `.github/PULL_REQUEST_TEMPLATE.md` updated with: backlog task ID field, cross-platform-smoke checkboxes (Mac smoke / Win smoke / Playwright / cargo test), AI-disclosure section, CHANGELOG-updated checkbox.
- Existing legal boilerplate preserved verbatim (exact byte-level match against the old version's lines 7-19, except for the project name once renamed).
- New PRs get the upgraded form pre-populated.

**Verification:** Open a PR (any change); confirm the new sections appear pre-filled. `diff` the legal section against the previous version to confirm verbatim preservation.

**Rename touch-points:** "OpenWhisper" appears 1× in the legal section.

## Task 6: `.github/dependabot.yml`

**Depends on TASK-82 landing first** so Dependabot PRs hit a working CI gate.

Author the file:

```yaml
version: 2
updates:
  # GitHub Actions — weekly. Action pinning has known security drift
  # (post-tj-actions/changed-files 2025 incident), so faster cadence
  # than language deps is justified.
  - package-ecosystem: github-actions
    directory: /
    schedule:
      interval: weekly
    open-pull-requests-limit: 5
    labels:
      - dependencies
      - github-actions

  # Cargo — monthly. Mid-cycle dep churn from a daily cadence wakes
  # the maintainer with PRs that consume CI minutes for trivial bumps.
  - package-ecosystem: cargo
    directory: /
    schedule:
      interval: monthly
    open-pull-requests-limit: 5
    labels:
      - dependencies
      - rust

  # npm (pnpm — Dependabot supports pnpm-lock.yaml since 2024) — monthly.
  - package-ecosystem: npm
    directory: /apps/tauri
    schedule:
      interval: monthly
    open-pull-requests-limit: 5
    labels:
      - dependencies
      - frontend
```

**Outcomes:**
- `.github/dependabot.yml` committed.
- Within a week of merge, GitHub starts opening Dependabot PRs for github-actions bumps; within a month, for cargo + npm bumps.
- Each Dependabot PR has labels (`dependencies` + ecosystem-specific) so maintainer can filter.
- TASK-82's CI workflow runs on each Dependabot PR; failing PRs are visibly red.

**Verification:** Wait for the first Dependabot PR (passive verification); confirm it opens against `main`, has the labels, and CI runs against it. Or trigger manually via GitHub Settings → Code security → Dependabot → "Check for updates".

**Rename touch-points:** none (Dependabot config is internal).

## Task 7: `CHANGELOG.md` seed

Author at repo root. Length: ~150 lines.

**Format:** Keep-A-Changelog 1.1 (`https://keepachangelog.com/en/1.1.0/`).

**Process:**

1. Read the existing release-handover docs:
   - `docs/release-0.3.0-handover.md`
   - `docs/release-0.4.0-handover.md`
   - `docs/release-0.5.0-handover.md`
2. For each release, extract the user-facing changes (NOT internal infra). Group as `Added` / `Changed` / `Fixed` / `Removed` / `Security` per Keep-A-Changelog conventions.
3. Reverse-walk so the most recent release is at the top.
4. Add an `## [Unreleased]` section at the very top with placeholder subheadings `### Added`, `### Changed`, `### Fixed`. This is what future PRs edit.
5. Footer: links to release tags via `[1.0.0]: https://github.com/<org>/<repo>/releases/tag/v1.0.0` etc. (currently only 0.x tags exist.)

Don't include internal-only changes (refactors, build infra, doc-only commits) — those belong in commit history, not the user-facing changelog. The handover docs themselves are kept as-is for the maintainer's archive.

**Outcomes:**
- `CHANGELOG.md` committed at repo root in Keep-A-Changelog format.
- Top section is `## [Unreleased]` with empty subheadings.
- Sections for 0.5.0, 0.4.0, 0.3.0 (in that order) follow, each with user-facing changes only, grouped by Added/Changed/Fixed.
- Footer has tag-comparison links (or placeholder URLs that the rename task fixes up).
- TASK-83.5 (PR template) checkbox "Updated CHANGELOG.md (or N/A)" exists, so going-forward enforcement is in place.

**Verification:** Reviewer reads each release section against the corresponding handover doc and confirms all user-visible changes are reflected (no fabrications, no internal noise).

**Rename touch-points:** `<org>/<repo>` URL in footer comparison links; project name in the file header.

## Cross-task verification checklist

Before marking TASK-83 done:

- [ ] All 7 subtasks `Done` in Backlog.
- [ ] `SECURITY.md`, `CODE_OF_CONDUCT.md`, `CHANGELOG.md` exist at repo root.
- [ ] `.github/ISSUE_TEMPLATE/{bug_report.yml, feature_request.yml, config.yml}`, `.github/CODEOWNERS`, `.github/PULL_REQUEST_TEMPLATE.md` (upgraded), `.github/dependabot.yml` exist.
- [ ] Maintainer email in `SECURITY.md` and `CODE_OF_CONDUCT.md` is project-owned (not personal).
- [ ] Existing legal boilerplate in PR template preserved verbatim.
- [ ] GitHub "Community Standards" check is green (Insights → Community Standards on the repo).
- [ ] GitHub Issue creation flow shows only the bug + feature templates, no "Open a blank issue" option.
- [ ] First Dependabot PR has opened (passive verification — may take up to a week).
- [ ] CHANGELOG `Unreleased` section exists and 0.5.0/0.4.0/0.3.0 historical entries are populated.
- [ ] Rename touch-points list compiled per task is handed to TASK-NEW-5 (the rename task) so the final sweep doesn't miss any.
