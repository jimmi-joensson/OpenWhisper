# OpenWhisper 0.3.0 release handover

Goal: ship OpenWhisper v0.3.0 — first multi-platform release of the Tauri shell.
Mac DMG ships first (built locally on macOS arm64). Windows MSI follows from a
separate Windows box; that box pulls the v0.3.0 tag and uploads its artifact
to the same GitHub Release. You are responsible for the Mac side end-to-end,
and for handing the Windows box a clear "your turn" pointer.

## Repo

`/Users/jimmijoensson/Repositories/OpenWhisper`

Single-shell repo: one Tauri app under `apps/tauri/`, Rust core under `core/`.
SwiftUI macOS shell archived to `archive/macos/` (reference only — do not build).
Backlog tracked via the Backlog.md CLI (`backlog task list`), not GitHub Issues.

## Starting state (verify before doing anything)

- Last release tag: `v0.2.0` (2026-04-24). 72 commits between v0.2.0 and main.
- Versions in `apps/tauri/package.json` + `apps/tauri/src-tauri/tauri.conf.json` +
  `apps/tauri/src-tauri/Cargo.toml` all currently say `0.1.0`. They drifted away
  from the v0.2.0 tag during the Tauri rewrite — bump them all to `0.3.0` in
  one commit before tagging.
- There is no GitHub Actions release workflow (the SwiftUI one was deleted with
  the apps/macos archive). TASK-46 tracks rebuilding it Tauri-native — do NOT
  block this release on TASK-46.

## Step 1 — Discover what 0.3.0 contains

Read the full commit list since v0.2.0:

```sh
git log v0.2.0..HEAD --oneline
git log v0.2.0..HEAD --stat                # for the "what files" view
```

Skim `backlog/tasks/` for tasks that landed between then and now (status: Done,
created_date after 2026-04-24). Also grep `git log v0.2.0..HEAD` for `TASK-` to
find the explicit task references.

Group commits into release-note buckets. Major themes you'll find:

- **Tauri shell**: ground-up rewrite of both Mac + Win shells into one codebase
  (`apps/tauri/`), retiring `apps/macos` (SwiftUI) and `apps/windows` (WinUI 3).
  "Tauri Phase 0…7" commits walk the build-out. Phases 0–3 = scaffold + main
  window parity. Phase 4 = global hotkey + Mac AX flow. Phase 5 = text
  injection (CGEventPost on Mac, SendInput on Win) + fullscreen detection.
  Phase 6 = single-instance, close-to-tray, mic + recognizer banners,
  accessory activation policy. Phase 7 = retire WinUI 3 + plan apps/macos
  retirement (TASK-41).
- **Recognizer**: engine swap to ort (TASK-40), per-host onnxruntime fetch
  script (`pnpm setup:ort`), env-tunable `num_threads`, RTX 3070 bench
  results that defer CUDA (TASK-39, TASK-40).
- **Pill HUD**: particle-morph + sphere transitions rework (TASK-42), new
  design-system handoff baked into `apps/tauri/src/PillOverlay.tsx`.
- **Transcript pipeline**: shared `core::transcript::process` now wired
  through both shells; new dedupe-adjacent-words pass (TASK-44) on top of
  the existing filler-strip + substitution + whitespace-normalize passes.
  Tauri shell now calls the same crate function the Mac shell did
  (TASK-43).
- **Repo hygiene**: `.gitattributes` LF pin, gitignore for `*.tsbuildinfo`,
  cross-platform `pnpm dev:tauri` wrapper (`dev-run.cjs`).
- **Polish**: every user-visible string (window title, tray menus + tooltip,
  hotkey + mic + recognizer banners) now derives from `productName` at
  runtime so dev/release variants render their own name. AX trust check
  uses silent-first → only fires the system modal when truly untrusted,
  eliminating the post-grant prompt loop.
- **Dev-run.sh release-coexistence**: dev cycle no longer kills the running
  release process or wipes its TCC grants. Dev + release coexist via
  distinct bundle ids (`com.openwhisper.app` vs `com.openwhisper.app.dev`).

Write a CHANGELOG entry (or release-notes draft) covering those buckets.
Don't paste raw commit subjects — group by user-visible feature/area, link
TASK-NN where one exists. Aim for ~400 words.

## Step 2 — Version bump commit

Bump `0.1.0` → `0.3.0` in:

- `apps/tauri/package.json`
- `apps/tauri/src-tauri/tauri.conf.json` (`"version"` field at top)
- `apps/tauri/src-tauri/Cargo.toml` (`[package]` version)
- `apps/tauri/src-tauri/Cargo.lock` (auto-updated by next cargo build)

Commit subject: `Repo: bump version to 0.3.0`. Body: brief — points at the
release notes you wrote in step 1.

## Step 3 — Build + test Mac release locally

Pre-reqs (one-time per host, may already be done):

```sh
cd apps/tauri && pnpm install
pnpm setup:ort                 # downloads onnxruntime dylib to
                               # ~/.cache/openwhisper/onnxruntime/
```

Build:

```sh
cd apps/tauri
PATH="$HOME/.cargo/bin:$PATH" pnpm release:mac
# → target/release/bundle/dmg/OpenWhisper_0.3.0_aarch64.dmg
# → target/release/bundle/macos/OpenWhisper.app
#
# `release:mac` runs `tauri build` then re-signs without hardened runtime.
# Hardened runtime + ad-hoc breaks CGEventTapCreate on Sequoia 15 — see
# scripts/sign-mac.cjs header.
```

Sanity checks:

```sh
pnpm test:ui                    # 10 Playwright cases must pass
cargo check                     # from apps/tauri/src-tauri — must pass too
```

Manual smoke — install the DMG locally and verify:

1. First-launch Gatekeeper bypass works (right-click → Open).
2. Menu-bar mic icon appears (no Dock icon — LSUIElement = true).
3. Mic + Accessibility prompts fire on first hotkey/record. AX prompt
   does NOT keep firing after grant (the silent-first check is what
   makes that work — regression flag).
4. Right Cmd tap starts recording, releases pill, transcribes, injects
   to focused field.
5. Close → window hides; quit only via tray.

Builds are ad-hoc signed (no Apple Developer account). End users do the
Gatekeeper bypass per INSTALL.md.

## Step 4 — Tag + GitHub Release

Once happy with the artifact:

```sh
git tag -a v0.3.0 -m "OpenWhisper 0.3.0 — Mac + Windows Tauri release"
git push origin main
git push origin v0.3.0
```

Create the release as a **DRAFT** (so the Windows box can attach its MSI before
it goes public):

```sh
gh release create v0.3.0 \
  target/release/bundle/dmg/OpenWhisper_0.3.0_aarch64.dmg \
  --draft \
  --title "OpenWhisper 0.3.0" \
  --notes-file <path-to-release-notes-from-step-1>
```

In the release notes, include a short "Install" section that points at
`INSTALL.md` for the Gatekeeper bypass walkthrough (the .dmg is ad-hoc
signed, not notarized — first-launch warning is expected).

## Step 5 — Hand off to Windows

Leave the release in DRAFT. Tell the user the Windows box's playbook:

On the Windows machine:

```sh
git pull
git checkout v0.3.0
cd apps\tauri
pnpm install                    # if dependencies changed
pnpm setup:ort                  # one-time per host (downloads the
                                # onnxruntime DLL to %LOCALAPPDATA%\…)
pnpm tauri build                # → target\release\bundle\msi\
                                #   OpenWhisper_0.3.0_x64_en-US.msi
                                # + nsis\OpenWhisper_0.3.0_x64-setup.exe
gh release upload v0.3.0 \
    target\release\bundle\msi\OpenWhisper_0.3.0_x64_en-US.msi
gh release edit v0.3.0 --draft=false   # publish only after both
                                        # platforms attached
```

Cross-compiling Win from Mac does NOT work (wry needs MSVC + WebView2
SDK, MSI bundling uses WiX). Don't try.

## Constraints / gotchas

- macOS Sequoia 15.x arm64 only (Apple Silicon). Intel Mac + Sonoma
  explicitly out of scope for MVP.
- Windows: x64 only. arm64 ports deferred.
- Backlog.md CLI for tasks (`backlog task list`, `backlog task create`).
  Don't open GitHub issues for tracking.
- Local working tree may have an uncommitted edit in
  `backlog/tasks/task-17 - Optional-LLM-cleanup-pass-for-transcript-repair.md`
  + an untracked `task-45 - Settings — Models board Ollama LLM bridge infra.md`.
  Those are the user's in-progress edits — leave them alone.
- The `.icns` currently in `apps/tauri/src-tauri/icons/` is a 39 KB single-size
  icon (ic13 only, 256@2x). Lower-fidelity than the SwiftUI shell shipped.
  A previous attempt to regen full-pyramid was rejected by the user as
  visually worse. Do NOT touch the icon unless explicitly asked — log
  a polish task instead.

## Report back

- Link to the draft GitHub release.
- Link to the version-bump commit + the `v0.3.0` tag.
- Confirm the manual smoke checklist (step 3) all green.
- Anything unexpected from step 1 you couldn't fit in the changelog.
