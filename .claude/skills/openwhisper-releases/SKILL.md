---
name: openwhisper-releases
description: Cut a versioned OpenWhisper release (Mac DMG + Windows MSI) split across two physical machines via a draft GitHub Release. READ before bumping versions, tagging, building release artifacts, or writing a release-handover doc. Triggers when the user asks to "cut a release", "release vN.M.0", "ship X.Y.Z", or "do the release handover".
---

# Cutting an OpenWhisper release

Releases are **split across two machines**: Mac DMG built on macOS arm64, Windows MSI built on a separate Windows x64 box. Cross-compile does NOT work — wry needs MSVC + WebView2 SDK on Win, hardened-runtime+adhoc breaks CGEventTapCreate on Mac. The two halves meet in a single **draft** GitHub Release, which is published only after both artifacts are attached.

There is no GitHub Actions release workflow yet. The whole pipeline runs locally per machine.

## The five-step shape

Every release follows the same shape. Don't skip; don't reorder.

1. **Discover scope** — `git log vPREV..HEAD` and group commits into user-visible buckets.
2. **Bump versions** in four manifests (one commit, before tagging).
3. **Build + smoke Mac side** locally, sign without hardened runtime.
4. **Tag, push, create DRAFT GH Release** with DMG attached.
5. **Hand off to Windows** via `docs/release-N.M.0-handover.md`. Win box pulls tag, builds MSI, uploads to the same draft, then publishes.

## Step 1 — Discover scope

```sh
git log vPREV..HEAD --oneline           # subjects
git log vPREV..HEAD --pretty=format:"%h %s" | grep -iE "TASK-"   # task refs
```

Skim `backlog/tasks/` for tasks with status `Done` and `updated_date` after the previous tag's date. Group commits into **user-visible buckets** (Settings, Pill, Hotkey, Permissions, etc.) — never paste raw subjects.

**Exclude internal-only buckets from notes.** Repo hygiene, skill writing, governance/license metadata, dev-tool changes — keep them out unless the user explicitly opts them in. Release notes are for end users, not contributors.

**Half-shipped features:** if a feature has UI but no backend wiring (e.g. a Switch that only updates local React state), exclude from notes. Don't advertise stubs.

## Step 2 — Bump versions

Bump `vPREV` → `vNEW` in all four:

- `apps/tauri/package.json`
- `apps/tauri/src-tauri/tauri.conf.json` (`"version"` at top)
- `apps/tauri/src-tauri/Cargo.toml` (`[package]` version)
- `core/Cargo.toml` (`[package]` version)

`Cargo.lock` updates on next `cargo build` — verify it picked up the new version after the build, then stage it.

Commit subject: `Repo: bump version to N.M.0`. Body: one line pointing at the release-notes doc.

## Step 3 — Build + smoke Mac

Pre-reqs (one-time per host, usually already done):

```sh
cd apps/tauri && pnpm install
pnpm setup:ort                    # → ~/.cache/openwhisper/onnxruntime/
```

Build:

```sh
cd apps/tauri
PATH="$HOME/.cargo/bin:$PATH" pnpm release:mac
# → target/release/bundle/dmg/OpenWhisper_N.M.0_aarch64.dmg
# → target/release/bundle/macos/OpenWhisper.app
```

**Use `pnpm release:mac`, not raw `pnpm tauri build`.** `release:mac` runs `tauri build` then re-signs without hardened runtime via `scripts/sign-mac.cjs`. Hardened runtime + ad-hoc signing breaks `CGEventTapCreate` on Sequoia 15 (the global hotkey hook stops firing). Verify after build:

```sh
codesign -d --entitlements - --xml target/release/bundle/macos/OpenWhisper.app 2>&1 | grep -i flags
# expect: flags=0x2 (or no hardened-runtime flag at all). NOT 0x10000 (runtime)
```

Sanity:

```sh
pnpm test:ui                      # all Playwright cases must pass
( cd src-tauri && cargo check )   # must pass
```

Manual smoke — install the DMG locally, verify against this checklist:

1. First-launch Gatekeeper bypass works (right-click → Open).
2. Menu-bar mic icon appears (no Dock icon — LSUIElement = true).
3. Mic + Accessibility prompts fire on first hotkey/record. AX prompt does NOT keep firing after grant (silent-first AX check is the regression flag).
4. Hotkey starts recording, releases pill, transcribes, injects to focused field.
5. Close → window hides; quit only via tray.
6. Anything new since vPREV — exercise each user-visible feature on the manual list.

## Step 4 — Tag, push, draft release

```sh
git tag -a vN.M.0 -m "OpenWhisper N.M.0 — <one-line theme>"
git push origin main
git push origin vN.M.0

gh release create vN.M.0 \
  target/release/bundle/dmg/OpenWhisper_N.M.0_aarch64.dmg \
  --draft \
  --title "OpenWhisper N.M.0" \
  --notes-file docs/release-N.M.0-handover.md   # or a notes-only file
```

**Always `--draft`.** The Win box hasn't attached its MSI yet; a non-draft release is publicly visible the instant you create it. Draft is the safety net — equivalent to "release as PR" since drafts are invisible until explicitly published.

In the notes, include a short **Install** section pointing at `INSTALL.md` for the Gatekeeper bypass walkthrough — DMGs are ad-hoc signed, not notarized.

## Step 5 — Hand off to Windows

Write `docs/release-N.M.0-handover.md`. Structure mirrors `docs/release-0.3.0-handover.md` — copy that as a starting point, update the version + scope. The doc should be **self-contained**: the Win box's agent has none of this conversation's context.

Required sections:

- **Repo path on the Win box** (paste-ready, not "your repo")
- **Starting state** — what tag to pull, what versions to expect
- **Scope summary** — same buckets as the Mac smoke, so the Win agent knows what to manually exercise
- **Build command** — `cd apps\tauri && pnpm install && pnpm setup:ort && pnpm tauri build`
- **Artifact paths** — `target\release\bundle\msi\OpenWhisper_N.M.0_x64_en-US.msi` and the `nsis\…-setup.exe`
- **Upload + publish** — `gh release upload vN.M.0 …` then `gh release edit vN.M.0 --draft=false`
- **Win-specific gotchas** — vendor-natives prereq (covered in `openwhisper-dev-workflow`), which exact features to smoke
- **Constraints** — x64 only, Win 11, what's *not* in scope this release

Do NOT tell the Win box to publish before manual smoke + Mac confirmation. The publish step always belongs to the user, not the agent.

## After both halves attach

User publishes the draft (or instructs the Win agent to). At that point:

- Tag is public, artifacts are public.
- Update `INSTALL.md` if the install flow changed.
- Close any TASK-NN entries the release relied on.

## Constraints / gotchas

- **macOS arm64 only** (Apple Silicon, Sequoia 15+). Intel + Sonoma out of scope.
- **Windows x64 only**. arm64 deferred.
- **Don't touch `apps/tauri/src-tauri/icons/icon.icns`** unless explicitly asked. The current single-size 256@2x icon was kept after a regen attempt was rejected as visually worse. Log a polish task instead.
- **Don't run `lsregister -kill -r`** as TCC cleanup — it wipes System Settings. (See `openwhisper-platform-gotchas`.)
- **Backlog tasks**, not GitHub Issues. Update task statuses + ACs as part of the release.
- **Cross-compile doesn't work** in either direction. Two physical machines, period.
- **Ship BOTH `.msi` and `.exe` (NSIS) on Windows.** Tauri's Windows builder produces both by default; we keep both attached deliberately. NSIS is friendlier for consumer per-user installs (smaller, no admin prompt); MSI is what enterprise / group-policy deploys want. Don't constrain `bundle.targets` in `tauri.conf.json` to drop one — the choice is intentional.
