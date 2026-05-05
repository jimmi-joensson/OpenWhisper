---
id: doc-32
title: Rename sweep — design
type: spec
created_date: '2026-05-04 16:31'
---

# Rename sweep — design

**Backlog parent:** TASK-85
**Milestone:** m-1 — v1.0 public release readiness
**Source research:** `backlog/docs/v1-oss-readiness-research.md` § "Rename candidates" + § "OpenWhispr (`OpenWhispr/openwhispr`)"

## Problem

`OpenWhispr/openwhispr` is an actively-maintained ~3k-star open-source local-dictation app (Electron + React + TypeScript, whisper.cpp + sherpa-onnx + llama.cpp). 80% functional overlap with OpenWhisper. Phonetic distance: one letter. Visual distance: one letter. The two names will be conflated in search engines, GitHub topic searches, package indexes, and human conversation forever once both are public.

The window to rename costs nothing externally today (OpenWhisper is private; no users, no incoming links, no SEO equity). After public flip, every uncorrected day costs: Google indexes, contributors file PRs against the old name, package managers register the old slug, social handles get squatted on the wrong spelling.

The decision is unilateral — the maintainer picks the new name. The work is mechanical: find every "OpenWhisper" reference and translate it under the new name in a single coordinated PR.

## Goal

After this task ships, the project has:

1. **One name everywhere** — repo, crates, bundle ids, settings paths, in-app strings, docs, social handles, domain. No half-renamed surfaces.
2. **No data loss for existing users** — anyone who installed v0.5.x has their settings and history preserved by a one-time migration shim.
3. **No TCC regrant** — Mac users who granted Accessibility/Microphone don't have to redo the dance because Team ID `898R9M89GU` stays stable across the bundle-id change.
4. **No search-engine confusion** — old `github.com/jimmi-joensson/OpenWhisper` URL redirects to the new repo for anyone with the link cached.
5. **Backlog history preserved** — task titles, plan docs, and decision records that say "OpenWhisper" stay as-is. They're historical references to what the project was called when those records were written, not forward-looking strings that can rot.

## Non-goals

- **Linux release rename.** Out of v1 scope; doesn't exist yet.
- **Code-level architecture changes** — this is a name change, not a refactor. If a contributor wants to also restructure the workspace during the rename, that's a separate PR.
- **Trademark filing.** Costs ~$300 in the US, several months of paperwork, and isn't load-bearing for v1. The maintainer can choose to file later under the new name.
- **Brand assets (logo, wordmark)** — the existing pill HUD and identity tokens stay. Re-skinning is a future task; the rename doesn't depend on it.
- **Migrate existing GitHub Issues / PRs / Releases.** GitHub repo-rename within the same user keeps all of these. If the maintainer changes user/org during the rename, that's a separate complexity flagged in Task 9.
- **Backfill the rename into the `0.5.x` patch series.** v0.5.x ships under "OpenWhisper". v1.0.0 ships under the new name. Any 0.5.x patch release between now and v1 stays "OpenWhisper".

## Behavior model

```
Pre-rename state (today):
  github.com/jimmi-joensson/OpenWhisper
  Cargo: openwhisper-core, openwhisper-tauri  (and openwhisper-cli post-TASK-81)
  Bundle: com.openwhisper.app  (Mac)
          openwhisper           (Win MSI product name)
  Settings: ~/Library/Application Support/com.openwhisper.app/
            %APPDATA%\openwhisper\
  Logs subsystem: com.openwhisper.OpenWhisper
  Skills: .claude/skills/openwhisper-*/
  Docs: README/INSTALL/BUILD/CONTRIBUTING/NOTICE — all reference "OpenWhisper"
                          │
                          │ TASK-85 single coordinated PR
                          ▼
Post-rename state:
  github.com/<new-org-or-user>/<new-name>     (old URL auto-redirects within same user)
  Cargo: <new>-core, <new>-tauri, <new>-cli
  Bundle: com.<new>.app   (Team ID 898R9M89GU stays — TCC grants survive)
          <new>            (Win MSI product name)
  Settings: ~/Library/Application Support/com.<new>.app/   (migrated from old by shim)
            %APPDATA%\<new>\                                 (migrated from old by shim)
  Logs subsystem: com.<new>.<new-name>
  Skills: .claude/skills/<new>-*/   (file-level rename + frontmatter `name:` updated)
  Docs: rewritten where text appears; backlog history left as-is
```

The rename is a tree of substitutions across forward-looking content. Backlog history is the one place where "OpenWhisper" is preserved verbatim — those tasks were written when the project was called OpenWhisper, and rewriting them would be retconning the historical record.

## Trade-offs

| Choice | Alternative | Why this |
|---|---|---|
| **Single coordinated PR**, multiple commits (one per subtask) | Many PRs, one per subtask, merged sequentially | Multi-PR has a worst-case where main sits half-renamed (e.g. Cargo crates renamed but bundle id not) for hours/days. CI gates each subtask but main is bisectably broken until the final PR lands. Single PR keeps the tree consistent. Cost: large diff (~144 files); reviewer fatigue. Mitigation: each commit in the PR is one subtask, reviewable independently in `git log -p` even though the merge is atomic. |
| **Bundle id changes** (`com.openwhisper.app` → `com.<new>.app`) | Keep the old bundle id forever as a "stable identifier" | A bundle id named after the wrong product creates the worst rename outcome: the *internal* identifier pretends OpenWhisper is still a thing while every user-facing surface says <new-name>. Future contributors hit this trap and push back. Bite the bullet now; TCC grants survive Team-ID-keyed signing per the platform-gotchas skill. |
| **Migration shim ships in this PR**, not as a follow-up | Defer migration to a v1.1 patch | Without the shim, every existing v0.5.x user upgrading to v1.0 silently loses their settings. That's hostile and the kind of breakage that lands on Hacker News. Inline shim is ~50 lines of Rust; shipping it in the rename PR pairs the breakage with its fix. |
| **Settings dir prefix follows bundle id** (`com.<new>.app/`) | Settings dir uses bare project name (`<new>/`) | Bundle-id-prefixed dirs are the macOS convention (matches what other notarized apps ship); CFBundleIdentifier and the support-dir name should agree. Cost: longer path. Worth it. |
| **Retain old dir for 60 days, then remove** | Wipe old dir immediately on first migration | If the migration shim has a bug, the user can recover by hand from the old dir. After 60 days of successful migrations across the user base, the cleanup is safe. Plan instructs the executor to file a v1.1 follow-up task to remove the cleanup branch after that grace period. |
| **Log subsystem id changes** (`com.openwhisper.OpenWhisper` → `com.<new>.<new-name>`) | Keep old subsystem id for log-archive continuity | Log archives older than the rename were written under the old subsystem; they don't need to be readable under the new name. Forward-looking logs use the new id. The continuity argument loses to the consistency argument. |
| **Skill file rename**: rename `.claude/skills/openwhisper-*/SKILL.md` files + their `name:` frontmatter | Keep the file paths + `name:` field as-is and only update body content | Frontmatter `name:` is the auto-loader's lookup key; if the project is renamed but skill names start with the old prefix, the auto-loader pattern in CLAUDE.md (`prefixed openwhisper-*`) gets out of sync. Fully rename. CLAUDE.md updates the prefix rule too. |
| **Backlog history left as-is** | Retroactively rewrite all task titles + plan docs | Backlog tasks are records of what was decided when. A task titled "Tauri Phase 0 — Scaffold app" doesn't get retconned into something else when the project is renamed. The decision doc Task 1 produces makes the rename itself legible to future readers; Backlog history then serves as a record of "what this project was called for tasks 1-85". |
| **Decision doc lives in backlog/decisions/**, not as a PR description | Use the rename PR description as the only record | PR descriptions get hidden behind a merge button; decision docs live in-repo and are searchable forever. Backlog already uses decision-N for ADRs. Use the same channel. |

## Risk register

- **Notarization breaks on bundle-id change.** Apple's notarization service treats `com.<new>.app` as a fresh app — first notarization request may take longer than usual; rare cases of stuck submissions. **Mitigation:** smoke notarization on a throwaway version (v0.99.0) before tagging v1.0.0 final. Test plan named in Task 4.
- **TCC grants don't actually survive.** Per the platform-gotchas skill, Team ID is the trust anchor on signed builds. But the same skill notes Debug builds remain ad-hoc-signed and DO drift TCC identity on every rebuild. So the production rename keeps grants, but Debug-build dev cycles after the rename hit the same TCC reset machinery (`version_reset.rs`) — which is fine, that's what it's for. **Mitigation:** verify TCC grants survive on a signed Release-channel build before declaring the rename done.
- **Settings migration shim races with first launch.** If the user double-launches the app immediately after upgrade, the second instance might race the first's migration. **Mitigation:** migration uses an atomic marker file write (`.migrated-from-openwhisper-v1`); second instance sees the marker and skips the copy. Single-instance lock (TASK-37) is also active.
- **GitHub auto-redirect doesn't fire** if the rename also changes the user/org name. GitHub redirects `OLD_USER/OLD_REPO` → `NEW_USER/NEW_REPO` only within the same user. If the maintainer creates a new org under the new name, old URLs 404. **Mitigation:** Task 9 documents this trade-off; if user-rename is required, plan adds a one-line README at `jimmi-joensson/OpenWhisper-archive` (a fresh repo at the old slug) pointing readers at the new repo.
- **OPENWHISPER_VERBOSE / OPENWHISPER_ORT_VERSION env vars rename**. These appear in `dev-run.cjs`, `fetch-ort.cjs`, and possibly user `.zshrc` files. Renaming env vars is a breaking change for any user who has them set. **Mitigation:** new env var names with a backwards-compatible read of the old names for one minor release; deprecation warning in verbose log if the old name is set; remove the read in v1.1.
- **macOS Spaces / launchctl re-registration.** When the bundle id changes, launchctl loses the existing "Open at Login" registration. **Mitigation:** if the user had Open at Login enabled, the migration shim re-registers under the new bundle id. Document in INSTALL.md upgrade section.
- **Memory directory path** lives at `~/.claude/projects/-Users-jimmijoensson-Repositories-OpenWhisper/memory/` — derived from the on-disk repo path. If the maintainer renames their local checkout directory after the GitHub rename, the memory path changes. **Mitigation:** explicitly call out as a maintainer step in Task 9; do NOT attempt to migrate memory entries automatically (they're machine-local, machine-state).
- **Rename merge conflicts with in-flight PRs.** If TASK-81/82/83/84 PRs are still open when TASK-85 merges, every one of those PRs hits a 100+ line conflict. **Mitigation:** sequence m-1 so TASK-81/82/83/84 are all merged before TASK-85 starts. Plan calls this out as a hard ordering dependency.

## Cross-task dependencies (hard)

- **TASK-81 must merge before TASK-85** — the rename touches the new `cli/` crate.
- **TASK-82 must merge before TASK-85** — the CI workflow file gets renamed paths.
- **TASK-83 must merge before TASK-85** — SECURITY.md / CoC / CHANGELOG / etc. all need a rename pass.
- **TASK-84 must merge before TASK-85** — README, BUILD.md, architecture.md all need a rename pass.

In other words: TASK-85 is the final task in m-1, and it doesn't start until everything else lands. Trying to interleave is asking for merge conflicts.
