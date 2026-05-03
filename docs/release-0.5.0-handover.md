# OpenWhisper 0.5.0 release handover

Goal: ship OpenWhisper v0.5.0. Mac DMG built locally on macOS arm64 + uploaded
first; Windows MSI follows from a separate Windows box, which pulls the v0.5.0
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

- Last release tag: `v0.4.0` (2026-04-30). 52 commits between v0.4.0 and main.
- Versions in `apps/tauri/package.json` + `apps/tauri/src-tauri/tauri.conf.json`
  + `apps/tauri/src-tauri/Cargo.toml` + `core/Cargo.toml` all bumped to `0.5.0`
  in the version-bump commit on `main` ahead of tagging.
- `Cargo.lock` will pick up the new core version on the first build — stage it
  as part of the same commit if it was missed on the Mac side.
- The Mac side has already built + signed + notarized + smoked the DMG, tagged
  `v0.5.0`, pushed the tag, and created a **draft** GitHub Release with the
  DMG attached. Your job is to attach the MSI + NSIS exe and (after the user
  OKs it) flip draft → published.

## What's in 0.5.0 (scope summary)

User-visible changes since v0.4.0, grouped by area:

- **Home pane + outer sidebar nav** (TASK-65) — main window now lands on a new
  Home pane with a live hotkey hint and a latest-transcript row (hover to
  copy). Outer sidebar nav (Home / Settings / Diagnostics) replaces the old
  flat layout; the sidebar swaps to settings panes when you enter Settings,
  and resets to General on exit.
- **Windows custom titlebar** (TASK-68) — Slack-style continuous dark panel
  across sidebar + titlebar; OS chrome dropped; native min/max/close buttons
  rendered in-app.
- **Audio ducking + pause during dictation** (TASK-61) — playback in other
  apps drops in volume (or pauses, configurable) while you dictate, then
  resumes when the pill closes. Mac uses adaptive polling (no slider needed);
  Windows uses the system MediaController + a user-configurable Bluetooth
  resume delay slider for headsets that take a moment to come back.
- **Pill 2× scale during record/transcribe** (TASK-70) — spring-driven scale
  tween on the pill HUD when recording/transcribing, with a
  `prefers-reduced-motion` fallback and a backdrop-blur counter-scale fix.
- **Launch at login wired** (TASK-60) — the General-pane "Launch at login"
  Switch now actually persists and auto-starts the app on login (was a UI
  stub in 0.4.0). Backed by `tauri-plugin-autostart`.
- **Mac hotkey regrant reliability** (TASK-69) — after re-granting
  Accessibility, the app re-launches via `open -n` instead of
  `app.restart()`, which preserves the launchctl registration the system
  hotkey path relies on.
- **Icon polish** — settings-pane icons + back arrow now use lucide
  (was unicode/emoji), titlebar right padding tightened on Windows so the
  close button sits flush.

## Step 1 — Pull the tag

```sh
git fetch --tags
git checkout v0.5.0
```

Confirm the four manifests show `0.5.0` (`apps/tauri/package.json`,
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

- `target\release\bundle\msi\OpenWhisper_0.5.0_x64_en-US.msi`
- `target\release\bundle\nsis\OpenWhisper_0.5.0_x64-setup.exe`

Cross-compiling Win from Mac does NOT work (wry needs MSVC + WebView2 SDK, MSI
bundling uses WiX). The vendor-natives prereq is automatic — `tauri.conf.json`
chains `pnpm vendor:natives` before the build to copy WebView2Loader.dll +
onnxruntime.dll next to the exe (see `openwhisper-dev-workflow` skill for why).

## Step 3 — Smoke the install

Install the MSI on the build box. Verify against this checklist before
uploading:

1. App launches; mic icon appears in tray; main window follows.
2. **Custom titlebar**: window has the Slack-style continuous dark sidebar +
   titlebar (no Windows OS chrome). Min / max / close buttons in the top-right
   work; close button sits flush against the right edge.
3. **Outer sidebar nav**: Home pane is the landing route, with a live hotkey
   hint hero and a latest-transcript row (hover → copy). Click Settings → the
   sidebar swaps to settings-pane links; click back → returns to outer nav and
   resets the settings pane to General on next entry.
4. Hotkey starts recording; pill shows + tweens (2× spring scale during record
   / transcribe); transcribes; injects to focused field. Try in Notepad, Edge,
   an Electron app (VS Code or Slack), and Chrome.
5. Esc cancels recording without bleeding to the focused app.
6. **Audio ducking**: start music in Spotify / browser / WMP; trigger hotkey;
   playback ducks/pauses; release hotkey → playback resumes. Try over a
   Bluetooth headset and tune the BT resume delay slider in Settings →
   General if resume is choppy.
7. **Launch at login**: toggle the Switch in Settings → General; reboot (or
   sign out / in); confirm the app auto-starts hidden in the tray. Toggle
   off; reboot again; confirm it does NOT auto-start.
8. Right-click tray → Preferences opens Settings; rebind a hotkey; new
   accelerator appears in the tray "Preferences…" label.
9. Settings → Audio: change input device; live meter responds when toggle on;
   pull mic, app falls back without crashing.
10. Settings → General → Behavior: toggle "Show in fullscreen apps" — pill
    hides on entering a real fullscreen app; ignore maximized-but-not-
    fullscreen.
11. Multi-monitor (if available): pill follows the cursor between monitors.
12. Theme picker: System / Light / Dark applies immediately, persists across
    relaunch, no FOUC on boot.

## Step 4 — Upload + (await) publish

```cmd
gh release upload v0.5.0 ^
    target\release\bundle\msi\OpenWhisper_0.5.0_x64_en-US.msi ^
    target\release\bundle\nsis\OpenWhisper_0.5.0_x64-setup.exe
```

**Do NOT publish.** Leave the release in draft until the user explicitly says
to go public. The publish command, once they OK it:

```cmd
gh release edit v0.5.0 --draft=false
```

## Constraints / gotchas

- **Windows 11 x64 only.** arm64 deferred.
- **Don't propose simplifying the vendor-natives step** — see the
  `openwhisper-dev-workflow` skill. The WebView2Loader.dll + onnxruntime.dll
  copy-next-to-exe step is non-negotiable for end-user installs on the
  GNU-toolchain build.
- **Ship BOTH `.msi` and `.exe` (NSIS)** — keep both attached deliberately.
  NSIS is friendlier for consumer per-user installs; MSI is what enterprise
  / group-policy deploys want.
- **Backlog.md CLI for tasks** — don't open GitHub issues for tracking, even
  if you find rough edges during smoke. File a backlog task instead.
- **Don't touch the Mac DMG** that's already attached. If you re-sign or
  rebuild the Mac side, the user has to redo their smoke checklist.
- **Audio ducking is in scope this release.** If WMP / Edge / Spotify don't
  duck on Windows, that's a release blocker — not a follow-up.

## Report back

- Confirm the smoke checklist (step 3) is all green.
- Link to the draft GitHub release showing both DMG + MSI + NSIS exe attached.
- Anything unexpected from the build — vendor-natives churn, WiX warnings,
  smoke regressions — call out before the publish step.
