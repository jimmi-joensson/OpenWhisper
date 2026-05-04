---
id: doc-33
title: Rename sweep — implementation plan
type: plan
created_date: '2026-05-04 16:31'
---

# Rename sweep — implementation plan

**Backlog parent:** TASK-85
**Spec:** `backlog/docs/specs/doc-32 - Rename-sweep-—-design.md`
**Milestone:** m-1 — v1.0 public release readiness

## Hard prerequisites

TASK-81, TASK-82, TASK-83, TASK-84 must all be merged to `main` before TASK-85 starts. Interleaving guarantees merge conflicts on a 144-file diff.

## Single-PR shape

The whole rename ships as **one PR** with **9 commits**, one per subtask, in numerical order. Reviewer reads `git log -p` to see each subtask in isolation; merge is atomic so `main` never sits half-renamed.

## Parameters (resolved at execution time)

The plan refers to these as bash-style placeholders. The maintainer fills them in once at the start of execution:

| Placeholder | Example | Used in |
|---|---|---|
| `<NEW_NAME>` | `Murmur` | Display strings, README title, bundle CFBundleName |
| `<new_name>` | `murmur` | Cargo package names (snake_case underscore replaced by hyphen for crate names: `<new>-core` → e.g. `murmur-core`), domain `<new>.dev` |
| `<new>` | `murmur` | bundle id suffix `com.<new>.app`, env var prefix `<NEW>_VERBOSE`, log subsystem `com.<new>.<new>` |
| `<new-cargo>` | `murmur` | crate names (`<new-cargo>-core`, `<new-cargo>-tauri`, `<new-cargo>-cli`). For most names this equals `<new>`; if the chosen name has hyphens or unusual casing it diverges. |
| `<new-org-or-user>` | `jimmi-joensson` (same user) or `murmurapp` (new org) | GitHub URL slug. Default = same user; if maintainer creates an org, this changes. |

In most plausible name picks (Murmur / Parley / Tellur), the four lower-case forms collapse to one value. Plan tasks refer to the placeholders for clarity; the executor's first action is to grep-replace them with the real values once.

## Task 1: Name pick + decision doc

**Maintainer-driven decision.** No code in this commit.

Steps:

1. Maintainer picks the new name from the research candidates (`backlog/docs/v1-oss-readiness-research.md` § Rename candidates: Murmur / Parley / Tellur are the recommended top-3; maintainer can also pick a fresh name).
2. Verify external availability for the chosen name (does NOT register yet — that's Task 2):
   - GitHub user/org slug available
   - `<new>.dev` and `<new>.com` domain availability (whois)
   - npm package name available (`npm view <new>`)
   - Brew cask name available (`brew search --cask <new>`)
   - Winget package id available (`winget search <new>`)
   - Social handles unclaimed (X, Bluesky, Mastodon, GitHub)
3. Run `backlog decision create "Rename to <NEW_NAME>" -s accepted` to land the ADR. Edit body to include:
   - Why the old name is wrong (one sentence: phonetic conflict with OpenWhispr)
   - Why this name (a paragraph rationalizing the choice)
   - The full placeholder mapping table (`<NEW_NAME>`, `<new_name>`, etc.) so future readers know the substitution rules
   - List of namespace registrations attempted, with status (claimed / pending / unavailable-fallback-name)

**Outcomes:**
- `backlog/decisions/decision-N - Rename to <NEW_NAME>.md` committed.
- ADR body explains the choice and lists namespace availability per channel.
- Placeholder mapping table embedded in the ADR for future readers / agents.
- No code changes in this commit.

**Verification:** `backlog decision list --plain` shows the new ADR. Reviewer reads the ADR body and confirms the namespace-availability list is concrete (not "we'll figure it out").

## Task 2: External namespace reservation checklist

**Maintainer-driven, no code in this commit.** The checklist itself ships as a doc; executing it is manual.

Author `docs/maintainer/rename-namespace-checklist.md` listing every external namespace the maintainer must claim, in priority order. Each line has a status checkbox:

```markdown
# Namespace reservation checklist for <NEW_NAME>

Critical (claim before any public mention):
- [ ] GitHub repo slug `<new-org-or-user>/<new_name>`  — claimed via repo rename or new org
- [ ] Domain `<new>.dev`  — registered, DNS pointed at GitHub Pages or static host
- [ ] Twitter/X handle `@<new>app` or `@<new>`
- [ ] Bluesky handle `<new>.<new>.dev`
- [ ] Mastodon handle (project's choice of instance)
- [ ] GitHub topic `<new>` (just commit a PR with the topic; auto-claimed)

Pre-launch (claim before v1.0 release):
- [ ] Brew cask name `<new>`
- [ ] Winget package id `<new>.<new>`
- [ ] npm package name `<new>` (squat with empty package; full publish post-rename)
- [ ] Cargo crate names `<new-cargo>-core`, `<new-cargo>-tauri`, `<new-cargo>-cli` (squat with empty crate; full publish post-rename)

Optional (claim if/when needed):
- [ ] LinkedIn page
- [ ] Reddit subreddit
- [ ] Hacker News submitter handle
```

The doc is checked in; the maintainer ticks boxes as registrations complete. Done state for the subtask = checklist exists; ticking is followup.

**Outcomes:**
- `docs/maintainer/rename-namespace-checklist.md` committed.
- Critical-section namespaces all claimed before Task 9 runs (the GitHub rename).
- Pre-launch namespaces claimed before v1.0 tag.
- Status visible in the doc's checkbox state.

**Verification:** Reviewer reads the checklist and confirms it covers every channel mentioned in the spec's risk register. Subsequent commits in the rename PR may proceed before all checkboxes are ticked, but Task 9 (GitHub repo rename) blocks on the Critical section being complete.

## Task 3: Cargo workspace rename

Rename Cargo packages, NOT directory paths (keeps git history continuous):

1. `core/Cargo.toml` — `name = "openwhisper-core"` → `name = "<new-cargo>-core"`. The `[lib] name = "openwhisper_core"` (snake_case) → `<new_cargo>_core`. Crate-types stay (`staticlib + rlib + cdylib`).
2. `apps/tauri/src-tauri/Cargo.toml` — `name = "openwhisper-tauri"` → `<new-cargo>-tauri`. Update `openwhisper-core` path-dep references in `[dependencies]`.
3. `cli/Cargo.toml` (post-TASK-81) — `name = "openwhisper-cli"` → `<new-cargo>-cli`; bin name `openwhisper` → `<new>`; `openwhisper-core` dep ref updated.
4. `scripts/bench/bench-sherpa/Cargo.toml` — update `openwhisper-core` path-dep ref if present.
5. **Imports**: every `use openwhisper_core::*` becomes `use <new_cargo>_core::*`. Use `rg -l 'openwhisper_core'` to find them; expect ~30-50 sites across `apps/tauri/src-tauri/` and `cli/`.
6. `Cargo.lock` — let cargo regenerate by deleting and running `cargo check --workspace`.
7. Root `Cargo.toml` — workspace `members = ["core", "apps/tauri/src-tauri", "scripts/bench/bench-sherpa", "cli"]` stays as-is (paths unchanged).
8. Workspace-level `[profile.dev.package.openwhisper-core]` override → `[profile.dev.package.<new-cargo>-core]`.

**Outcomes:**
- All three (post-TASK-81: four) workspace crate `name` fields use `<new-cargo>-*`.
- Every `use openwhisper_core::*` import replaced with `use <new_cargo>_core::*`.
- `Cargo.lock` regenerated and committed.
- `cargo check --workspace --exclude bench-sherpa --no-default-features --features tauri` passes on Mac and Windows.
- The `[profile.dev.package.<crate>]` override in root `Cargo.toml` updated to the new crate name (otherwise the dev-build optimization stops applying).
- Directory paths under `core/`, `apps/tauri/src-tauri/`, `cli/` unchanged — git history intact.

**Verification:** `cargo check --workspace`, `cargo build -p <new-cargo>-tauri`, `cargo build -p <new-cargo>-cli`. Confirm `target/release/<new>.exe` (Win) or `target/release/<new>` (Mac CLI) builds.

## Task 4: Mac bundle rename

Edit `apps/tauri/src-tauri/tauri.conf.json` and `tauri.dev.conf.json`:

1. `productName` → `<NEW_NAME>` (display).
2. `bundle.identifier` → `com.<new>.app`.
3. `app.windows[].title` (main window) → `<NEW_NAME>`. Pill window title stays empty.
4. `app.windows[].label` stays as `main` / `pill` (internal Tauri identifiers — DON'T change, would break window-find-by-label).
5. `bundle.icon` paths stay (just PNG files; rename is for content not filename).
6. `bundle.macOS.signingIdentity` stays — `Developer ID Application: Jimmi Joensson (898R9M89GU)`. Team ID is the trust anchor; bundle id can change underneath it.
7. `bundle.macOS.minimumSystemVersion` stays.
8. `apps/tauri/scripts/notarize-mac.cjs` — bundle id is **not** hardcoded, but two other strings ARE:
   - DMG filename pattern `OpenWhisper_${PKG.version}_aarch64.dmg` (line 6) → `<NEW_NAME>_${PKG.version}_aarch64.dmg`. The pattern reads `productName` from `tauri.conf.json` indirectly via `PKG`; if the new productName flows through, this auto-renames. Verify by inspection.
   - Keychain profile name `openwhisper-notarytool` (lines 34, 36) → `<new>-notarytool`. **Maintainer must also rename the keychain entry on their dev box** via `xcrun notarytool store-credentials <new>-notarytool ...` before the rename PR ships, OR the notarize step fails with "credential not found". Document in the rename-namespace-checklist (Task 2).
9. `apps/tauri/scripts/dev-run.sh` — verify it doesn't hard-code `OpenWhisper.app` or `com.openwhisper.app` paths. Update if so.
10. `apps/tauri/src-tauri/src/permissions/version_reset.rs` — **reads bundle id dynamically** from `app.config().identifier` (verified at `version_reset.rs:95`). **No edit required** — the new bundle id flows through automatically. **However**, version_reset.rs only clears stale rows for the *current* bundle id on cdhash drift; it will NOT clean up the legacy `com.openwhisper.app` rows in System Settings → Privacy & Security → Accessibility/Microphone. Those rows will linger as orphaned entries the user can remove manually, OR — better — add an explicit one-shot legacy-bundle-id cleanup that runs once on first launch under the new bundle id:
    ```rust
    // Pseudocode in lib.rs::setup() or version_reset.rs, gated by the
    // migration marker from Task 7 so it runs exactly once per upgrade.
    if migrate_legacy_settings_dir() == MigrationStatus::Migrated {
        for service in ["Accessibility", "Microphone", "ListenEvent"] {
            run_tccutil_reset(service, "com.openwhisper.app");
        }
    }
    ```
    The `tccutil reset` exit code is ignored (exit 1 = "no entries" = desired no-op).
11. `apps/tauri/src-tauri/src/permissions/mac.rs` — any hard-coded bundle id strings.

**Notarization smoke test before tagging v1.0.0**: cut a `0.99.0-rename-smoke` build, run the full release flow (`pnpm release:mac` → `pnpm notarize:mac`), verify Apple's notarization succeeds and the resulting DMG installs cleanly on a test Mac. If notarization sticks, escalate before v1.0.0 tag.

**Outcomes:**
- `tauri.conf.json` + `tauri.dev.conf.json` carry `productName = "<NEW_NAME>"` and `bundle.identifier = "com.<new>.app"`.
- Mac signing identity reference unchanged (Team ID `898R9M89GU` preserved).
- `version_reset.rs` references the new bundle id.
- A signed + notarized build of `0.99.0-rename-smoke` (or equivalent throwaway version) installs cleanly on a test Mac and Accessibility/Mic grants from a prior `com.openwhisper.app` install are NOT visible (TCC sees the new bundle id as fresh, but `version_reset.rs` clears any stale `com.openwhisper.app` rows).
- `pnpm release:mac` produces `<NEW_NAME>-0.99.0-arm64.dmg` (or whatever Tauri's filename template emits).

**Verification:** Notarization smoke per the test plan above. Manual install on test Mac; grant Accessibility + Mic; press hotkey; transcribe a sentence; confirm the pill HUD shows the new name in window title. Confirm `~/Library/Application Support/com.<new>.app/` is created.

## Task 5: Windows bundle rename

Edit `tauri.conf.json` (same file; Tauri 2 unifies bundle config across platforms with platform-specific overrides):

1. `bundle.windows.wix.productName` → `<NEW_NAME>`.
2. `bundle.windows.wix.upgradeCode` — **CURRENTLY ABSENT** in `tauri.conf.json` (no `bundle.windows` block exists). When `upgradeCode` is missing, Tauri/WiX auto-derives one from `productName` — which means the rename will produce a *new* upgradeCode, and existing v0.5.x users upgrading to v1.0 get a **side-by-side install** rather than an in-place upgrade. **Hard fix required**: ADD an explicit stable UUID `upgradeCode` to `tauri.conf.json` *before this PR ships*, derived from the OLD productName (so v0.5.x → v1.0 upgrades are recognized as the same product). Use any UUID generator (`uuidgen` on Mac); commit the resulting block:
    ```jsonc
    "bundle": {
        "windows": {
            "wix": {
                "upgradeCode": "<UUID-pinned-once-then-immutable>"
            }
        }
    }
    ```
    Once pinned, this UUID is **immutable forever** — every future Win release uses the same upgradeCode for the in-place-upgrade contract to hold. Document the pinning in the rename ADR (Task 1) so future maintainers don't accidentally regenerate it.
3. Install dir defaults to `%PROGRAMFILES%\<NEW_NAME>\` — controlled by Tauri's MSI generator from `productName`. Should auto-update.
4. `%APPDATA%` path: code that references it likely uses `dirs::data_dir()` → `~\AppData\Roaming\<bundle-id-or-productName>\`. **Verify**: search for hardcoded `OpenWhisper` strings in `apps/tauri/src-tauri/src/` for Windows-side path construction.
5. Add/Remove Programs label = `productName` automatically.
6. `apps/tauri/scripts/vendor-natives.cjs` — verify the script doesn't hard-code `openwhisper-tauri.exe` filename. Update if so.
7. Win MSI signing — TASK-66 hasn't shipped Win signing yet, so no signing identity to update.

**Outcomes:**
- MSI builds via `pnpm tauri build` produce `<NEW_NAME>-0.99.0-x64.msi`.
- Add/Remove Programs shows the new name on a test Windows install.
- `%APPDATA%\<new>\` (or whatever the new path is) gets created on first launch.
- `upgradeCode` is a stable UUID, not name-derived (so future MSI upgrades don't fork install paths).
- `vendor-natives.cjs` works without referencing the old exe name.

**Verification:** `pnpm tauri build` on a Windows host; install resulting MSI; verify Add/Remove Programs label; press hotkey; transcribe; confirm `%APPDATA%\<new>\` populated.

## Task 6: In-app strings sweep

Hunt every user-visible "OpenWhisper" string in code:

1. `apps/tauri/src-tauri/src/lib.rs` — `product_name()` fallback string `"OpenWhisper"` → `"<NEW_NAME>"`.
2. `apps/tauri/src-tauri/src/tray/mod.rs` — tray menu labels.
3. `apps/tauri/src-tauri/src/permissions/mac.rs` — error messages, dialog texts.
4. `apps/tauri/src/components/**.tsx` — header strips, settings labels, toast messages, h1 elements with `data-tauri-drag-region`. Search: `rg "OpenWhisper" apps/tauri/src/`.
5. `core/src/verbose.rs` — **No OSLog subsystem id today** (current implementation is plain `eprintln!` keyed on `OPENWHISPER_VERBOSE`, no `os_log` integration). The macOS `log stream --predicate 'subsystem == "com.openwhisper.OpenWhisper"'` invocation that appears in INSTALL.md is forward-looking aspiration, not current behavior. **No edit required to verbose.rs for the subsystem id.** If/when an OSLog subsystem is wired post-v1, it must use the new bundle id. Update INSTALL.md's example to reflect current reality (i.e., remove the `log stream` line, or replace with `tail -f apps/tauri/.openwhisper-verbose.log` — the actual verbose path).
6. `core/src/dictation.rs` — any user-facing status strings (per the orchestration-in-rust rule, status strings live in core; verify none mention the project name).
7. Error toasts in `apps/tauri/src-tauri/src/permissions/version_reset.rs` and elsewhere.
8. README badge alt text (Task 84.2 already authored these — they reference `<NEW_NAME>` if the rename happens after TASK-84 ships, but the badges shipped under "OpenWhisper"; this commit catches the alt-text drift).
9. Window titles set in JS via `getCurrentWindow().setTitle()`.
10. Env var names: `OPENWHISPER_VERBOSE` → `<NEW>_VERBOSE`, `OPENWHISPER_VERBOSE_LOG` → `<NEW>_VERBOSE_LOG`, `OPENWHISPER_ORT_VERSION` → `<NEW>_ORT_VERSION`. Backwards-compat read of old names with a deprecation log line for one minor release.

**Outcomes:**
- `rg "OpenWhisper|openwhisper" apps/tauri/src/ apps/tauri/src-tauri/src/ core/src/ cli/src/` returns zero results in code paths (only comments and historical references remain — those are fine).
- Tray menu, Pill, Settings, error toasts, window titles all show the new name when running a Mac smoke build.
- Verbose log subsystem id is the new id; `log stream --predicate 'subsystem == "com.<new>.<NEW_NAME>"'` produces output.
- Env vars: setting `<NEW>_VERBOSE=1` enables verbose mode; setting old `OPENWHISPER_VERBOSE=1` still works (with deprecation warning).

**Verification:** Build + run app; visually inspect every visible surface (tray, Pill, Settings, dialogs, error states); `log stream` for the new subsystem id; set old env var and confirm deprecation warning fires.

## Task 7: Settings/data migration shim

New file `core/src/settings/migration.rs` (or wherever the settings module lives post-TASK-81.2). Idempotent one-time migration on app boot:

```rust
pub fn migrate_legacy_settings_dir() -> Result<MigrationStatus, MigrationError> {
    let new_dir = dirs::data_dir()?.join("com.<new>.app");
    let marker = new_dir.join(".migrated-from-openwhisper");

    if marker.exists() {
        return Ok(MigrationStatus::AlreadyMigrated);
    }

    let old_dir = dirs::data_dir()?.join("com.openwhisper.app");
    if !old_dir.exists() {
        return Ok(MigrationStatus::NoLegacyData);
    }

    fs::create_dir_all(&new_dir)?;
    copy_dir_recursive(&old_dir, &new_dir)?;
    // Atomic marker write: ensures double-launch doesn't double-migrate
    fs::write(&marker, format!("migrated at {}\n", chrono::Utc::now()))?;

    // Schedule old-dir deletion for grace-period cleanup (60 days).
    // TODO(v1.1): remove after grace period — file follow-up subtask.
    Ok(MigrationStatus::Migrated)
}
```

Same shape on Windows: `%APPDATA%\openwhisper\` → `%APPDATA%\<new>\`.

Wire into `lib.rs::setup()` early — before the dictation engine touches the settings store.

**Outcomes:**
- `migrate_legacy_settings_dir()` runs once per fresh install at startup.
- A user with existing v0.5.x data at `~/Library/Application Support/com.openwhisper.app/` finds their settings + history at `com.<new>.app/` after the first v1.0 launch.
- A user without existing v0.5.x data sees `MigrationStatus::NoLegacyData` and migration is skipped.
- A user who launches v1.0 twice in quick succession sees the second instance hit `MigrationStatus::AlreadyMigrated` (atomic marker write prevents races).
- Verbose log records the migration event at INFO level.
- A v1.1 follow-up Backlog task is filed to remove the migration code after a 60-day grace period (cleanup of old dir + delete migration code).
- Mirror behavior on Windows.

**Verification:** Unit test in `core/src/settings/migration.rs` covers (a) no legacy dir → no-op, (b) legacy dir → copied, (c) marker exists → no-op. Manual smoke: place a fake settings file at `~/Library/Application Support/com.openwhisper.app/settings.toml`, launch v1.0 app, confirm file exists at `~/Library/Application Support/com.<new>.app/settings.toml` and the marker `.migrated-from-openwhisper` is present.

## Task 8: Docs sweep

The largest commit by line count, smallest by complexity. Find every forward-looking "OpenWhisper" reference in markdown / config / scripts:

1. **Root docs**: `README.md`, `INSTALL.md`, `BUILD.md` (TASK-84.3), `NOTICE`, `LICENSE` (verify if it names the project), `CONTRIBUTING.md`, `CHANGELOG.md` (TASK-83.7), `CLAUDE.md`, `SECURITY.md` (TASK-83.1), `CODE_OF_CONDUCT.md` (TASK-83.2).
2. **`.github/`**: `PULL_REQUEST_TEMPLATE.md` (legal section names the project — preserve structure, just substitute name; **do not** lose the "I retain all rights..." paragraph), `ISSUE_TEMPLATE/*.yml`, `dependabot.yml` (no project name in there, but verify), `CODEOWNERS`, `workflows/ci.yml`.
3. **`.claude/skills/`**: every `openwhisper-*` skill directory renamed to `<new>-*`. Frontmatter `name:` field updated to match. Body content updated where it references "OpenWhisper" forward-looking. **CLAUDE.md's rule** says "Prefer our own project skills (under `.claude/skills/` at repo root, prefixed `openwhisper-*`)" — change the prefix to `<new>-*`.
4. **`docs/`**: every file under `docs/` (architecture.md, design/*.md, spikes/*.md, the release-handover docs — the release-handover docs ARE historical and stay as-is, but other docs are forward-looking).
5. **`apps/tauri/package.json`**: `name`, `description`, `homepage`, `repository.url`, `bugs.url`. New URL = `github.com/<new-org-or-user>/<new_name>`.
6. **`apps/tauri/scripts/*.cjs`**: comment headers, log strings.
7. **`Cargo.toml` files** under each crate: `description`, `repository`, `homepage` if they reference the project.
8. **Backlog history**: **explicitly NOT touched.** Backlog tasks/decisions/plans/specs that reference "OpenWhisper" stay as-is. They're historical. The rename ADR (Task 1) makes the rename legible to future readers; existing Backlog entries serve as the record of the project's prior name.
9. **Skill rename specifics**: every reference to `.claude/skills/openwhisper-X/SKILL.md` in CLAUDE.md, in `backlog/docs/plans/*` (forward-looking ones), or in code comments needs the path updated. Use `rg ".claude/skills/openwhisper-"` to find them all.

**Outcomes:**
- `rg "OpenWhisper" --glob '!backlog/' --glob '!archive/' --glob '!docs/release-*-handover.md'` returns zero hits in forward-looking content.
- Same grep for lowercase `openwhisper` returns zero hits (modulo the env-var backwards-compat read in `verbose.rs`, which is intentional).
- Every `.claude/skills/openwhisper-*/` directory has been renamed to `<new>-*/`. The `name:` frontmatter in each skill's `SKILL.md` is updated.
- `CLAUDE.md`'s rule about skill prefix uses `<new>-*` not `openwhisper-*`.
- `apps/tauri/package.json` `homepage`/`repository.url`/`bugs.url` point at the new GitHub URL (will go live after Task 9 lands the actual rename).
- Legal boilerplate in PR template substitutes the project name only — the legal-rights paragraph structure is preserved verbatim.

**Verification:** Reviewer runs the `rg` commands above and confirms zero hits in forward-looking surfaces. Reviewer reads the legal section of `PULL_REQUEST_TEMPLATE.md` against TASK-83.5's preserved text and confirms only the project-name word changed.

## Task 9: GitHub repo rename + redirect

**Maintainer-driven; no code in this commit other than the URL fixups in dependent files.**

Steps (in order):

1. **Verify Task 2's Critical-section namespaces are claimed** before this step runs.
2. **Maintainer renames the GitHub repo**: GitHub Settings → Repository name → enter new name. If staying on the same user (`jimmi-joensson`), GitHub auto-redirects old URLs forever. If moving to a new org, manually communicate the move (no auto-redirect across user/org boundaries).
3. **Verify redirect**: `curl -I https://github.com/jimmi-joensson/OpenWhisper` returns 301 to the new URL. If user/org changed, this test will fail; document in INSTALL.md upgrade section that the old URL is dead.
4. **Update README badge URLs** to the new path (Task 84.2 used `<org>/<repo>` placeholders — substitute now). CI status badge, release-tag badge, anything else.
5. **Update `apps/tauri/package.json`** `homepage`, `repository.url`, `bugs.url` (Task 8 already changed the literal text; this is the verification that the URLs resolve).
6. **Update `dependabot.yml`** if any `directory` references the project name (today's dependabot.yml uses `/` and `/apps/tauri` paths; no project name in the path).
7. **Update `.github/ISSUE_TEMPLATE/config.yml`** `<org>/<repo>` placeholders (TASK-83.3).
8. **Update `CHANGELOG.md` footer** tag-comparison links (TASK-83.7).
9. **CODEOWNERS** stays — it uses `@<handle>`, not project URL.
10. **README repo-archive doc**: if the maintainer creates a new GitHub user/org rather than staying on `jimmi-joensson`, file an additional 2-line README at `jimmi-joensson/OpenWhisper-archive` (a fresh empty repo at the old slug) pointing readers at the new repo. Costs one minute, prevents permanent 404s.

**Outcomes:**
- The repo is at `github.com/<new-org-or-user>/<new_name>`.
- `curl -I https://github.com/jimmi-joensson/OpenWhisper` returns 301 (within-user rename) OR a documented archive-readme exists at the old slug.
- All README badge URLs work (CI badge resolves to a real workflow page).
- `package.json` `homepage`/`repository`/`bugs` URLs resolve to 200 OK.
- Issue template `config.yml` Discussions URL works.
- CHANGELOG.md tag-comparison links work.
- Maintainer notifies any external places that linked the old URL (Hacker News, blog, social).

**Verification:** Manual: visit each URL changed in this commit; confirm 200 OK or 301 → 200. Reviewer doesn't need to verify (it's runtime, not codebase-shaped).

## Cross-task verification checklist

Before marking TASK-85 done:

- [ ] All 9 subtasks `Done` in Backlog.
- [ ] `cargo check --workspace` clean on Mac and Windows.
- [ ] `pnpm test:ui` from `apps/tauri/` passes.
- [ ] CI workflow (TASK-82) green on the rename PR.
- [ ] Notarization smoke (`0.99.0-rename-smoke` Mac DMG) succeeded.
- [ ] Manual smoke: install signed v0.99.0-rename-smoke; grant Accessibility + Mic; record + transcribe a sentence; verify text appears.
- [ ] Migration smoke: place fake `~/Library/Application Support/com.openwhisper.app/settings.toml`; launch new build; verify file is now at the new path.
- [ ] `rg "OpenWhisper|openwhisper" --glob '!backlog/' --glob '!archive/' --glob '!docs/release-*-handover.md' --glob '!core/src/verbose.rs'` returns zero hits (verbose.rs is excluded for the env-var backwards-compat read).
- [ ] `.claude/skills/<new>-*/` exists; no `.claude/skills/openwhisper-*/` directories remain.
- [ ] CLAUDE.md prefix rule uses `<new>-*` not `openwhisper-*`.
- [ ] GitHub repo renamed; old URL returns 301 (or archive-readme exists).
- [ ] All forward-looking external URLs (badges, package.json, ISSUE_TEMPLATE config) resolve 200 OK.
- [ ] v1.1 follow-up task filed: remove migration shim and old-dir cleanup after 60-day grace period.
- [ ] v1.1 follow-up task filed: remove env-var backwards-compat read after one minor release.

After this checklist passes: tag `v1.0.0`, ship release artifacts, milestone m-1 closes.
