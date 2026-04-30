# OpenWhisper 0.4.0 release handover

Goal: ship OpenWhisper v0.4.0. Mac DMG built locally on macOS arm64 + uploaded
first; Windows MSI follows from a separate Windows box, which pulls the v0.4.0
tag and uploads to the same draft GitHub Release. The release is published only
after both artifacts are attached.

This doc is the **Windows-side handover** — the Win box's agent should be able
to read this top-to-bottom and complete its half without consulting the
conversation that produced it.

## Repo

`C:\Users\<you>\Repos\OpenWhisper` (or wherever your local clone lives)

Single-shell repo: one Tauri app under `apps/tauri/`, Rust core under `core/`.
Tasks tracked via Backlog.md CLI (`backlog task list`), not GitHub Issues.

## Starting state (verify before doing anything)

- Last release tag: `v0.3.0` (2026-04-27). 63 commits between v0.3.0 and main.
- Versions in `apps/tauri/package.json` + `apps/tauri/src-tauri/tauri.conf.json`
  + `apps/tauri/src-tauri/Cargo.toml` + `core/Cargo.toml` all bumped to `0.4.0`
  in the version-bump commit on `main` ahead of tagging.
- `Cargo.lock` will pick up the new core version on the first build — stage it
  as part of the same commit if it was missed on the Mac side.
- The Mac side has already built + smoked the DMG, tagged `v0.4.0`, pushed the
  tag, and created a **draft** GitHub Release with the DMG attached. Your job is
  to attach the MSI and (after the user OKs it) flip draft → published.

## What's in 0.4.0 (scope summary)

User-visible changes since v0.3.0, grouped by area:

- **Settings shell + panes** — sidebar shell layout (TASK-49); General pane w/
  Startup / Appearance / Updates sections built on shadcn primitives (TASK-56);
  Audio pane w/ Discord-style mic picker, persistent cpal-id selection, live
  device sync + disconnect fallback, opt-in level meter (TASK-53); Shortcuts
  pane w/ rebindable toggle + cancel hotkeys, in-window routed Settings view;
  Tray "Preferences…" entry with dynamic hotkey-accelerator label.
- **Pill multi-monitor support** (TASK-55) — pill follows active screen via
  cursor tracking; Dock-aware bottom-center placement; vertical-stack monitor
  topologies; cross-DPI positioning via per-platform `set_position`; main
  window follows pill across monitor changes; opt-out toggle in General pane.
- **Show in fullscreen apps** toggle (TASK-58) — NSPanel collection-behavior
  honors the setting on Mac; detector callback aborts on fullscreen-entry when
  off; cached frame cleared on hide. Windows fullscreen detector now correctly
  distinguishes maximized windows from real fullscreen (TASK-57), and skips
  WS_MAXIMIZE windows + chromeless screens.
- **Tray menu refresh** — status header + Show Main entry + ticking timer.
- **Theme picker** (System / Light / Dark) — wired through the General pane
  with a no-FOUC guard on boot (TASK-54 AC#4). Theme persists across launches.
  *Note:* the "Launch at login" Switch in the General pane is a UI stub in
  0.4.0 — backend autostart wiring lands later.
- **Hotkey + injection fixes** — Esc + start/stop no longer bleed to the
  focused app (#9); WebView2-focused hotkey path uses a JS keydown fallback on
  Windows; Electron/Chromium hosts no longer paste-lag on Windows injection.
- **Permissions UX** — main window comes forward at AX/mic prompt edges on
  Mac; stale TCC entries auto-reset on version change (TASK-48); recognizer
  EP-probe errors + download/extract progress now surface in the UI (#4).
- **Titlebar** — gear button to open Settings.

## Step 1 — Pull the tag

```sh
git fetch --tags
git checkout v0.4.0
```

Confirm the four manifests show `0.4.0` (`apps/tauri/package.json`,
`apps/tauri/src-tauri/tauri.conf.json`, `apps/tauri/src-tauri/Cargo.toml`,
`core/Cargo.toml`).

## Step 2 — Build the Windows release bundle

Pre-reqs (one-time per host):

```cmd
cd apps\tauri
pnpm install
pnpm setup:ort
```

Build:

```cmd
cd apps\tauri
pnpm tauri build
```

Expected artifacts:

- `target\release\bundle\msi\OpenWhisper_0.4.0_x64_en-US.msi`
- `target\release\bundle\nsis\OpenWhisper_0.4.0_x64-setup.exe`

Cross-compiling Win from Mac does NOT work (wry needs MSVC + WebView2 SDK, MSI
bundling uses WiX). The vendor-natives prereq is automatic — `tauri.conf.json`
chains `pnpm vendor:natives` before the build to copy WebView2Loader.dll +
onnxruntime.dll next to the exe (see `openwhisper-dev-workflow` skill for why).

## Step 3 — Smoke the install

Install the MSI on the build box. Verify against this checklist before
uploading:

1. App launches; mic icon appears in tray; main window follows.
2. Hotkey starts recording, pill shows + tweens, transcribes, injects to focused
   field. Try in: Notepad, Edge, an Electron app (e.g. VS Code or Slack), and
   Chrome — Electron/Chromium injection used to paste-lag pre-0.4.0.
3. Esc cancels recording without bleeding to focused app.
4. Right-click tray → Preferences opens Settings; rebind a hotkey; the new
   accelerator appears in the tray "Preferences…" label.
5. Settings → Audio: change input device; live meter responds (when toggle on);
   pull mic, app falls back without crashing.
6. Settings → General → Behavior: toggle Show in fullscreen — pill should hide
   when entering a real fullscreen app, ignore maximized-but-not-fullscreen.
7. Multi-monitor (if available): pill follows the cursor between monitors.
8. Theme picker: System / Light / Dark applies immediately, persists across
   relaunch, no FOUC on boot.

## Step 4 — Upload + (await) publish

```cmd
gh release upload v0.4.0 ^
    target\release\bundle\msi\OpenWhisper_0.4.0_x64_en-US.msi ^
    target\release\bundle\nsis\OpenWhisper_0.4.0_x64-setup.exe
```

**Do NOT publish.** Leave the release in draft until the user explicitly says
to go public. The publish command, once they OK it:

```cmd
gh release edit v0.4.0 --draft=false
```

## Constraints / gotchas

- **Windows 11 x64 only.** arm64 deferred.
- **Don't propose simplifying the vendor-natives step** — see the
  `openwhisper-dev-workflow` skill. The WebView2Loader.dll + onnxruntime.dll
  copy-next-to-exe step is non-negotiable for end-user installs on the
  GNU-toolchain build.
- **Backlog.md CLI for tasks** — don't open GitHub issues for tracking, even
  if you find rough edges during smoke. File a backlog task instead.
- **Don't touch the Mac DMG** that's already attached. If you re-sign or
  rebuild the Mac side, the user has to redo their smoke checklist.
- **The Launch-at-login Switch in Settings → General is a stub in 0.4.0.**
  Don't be surprised it doesn't persist. The backing autostart plugin work is
  not in this release.

## Report back

- Confirm the smoke checklist (step 3) is all green.
- Link to the draft GitHub release showing both DMG + MSI attached.
- Anything unexpected from the build — vendor-natives churn, WiX warnings,
  smoke regressions — call out before the publish step.
