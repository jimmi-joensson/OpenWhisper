# OpenWhisper 0.6.0 release handover

Goal: ship OpenWhisper v0.6.0. Mac DMG built locally on macOS arm64 + uploaded
first; Windows MSI + NSIS exe follow from a separate Windows box, which pulls
the v0.6.0 tag and uploads to the same draft GitHub Release. The release is
published only after both artifacts are attached.

This doc is the **Windows-side handover** — the Win box's agent should be able
to read this top-to-bottom and complete its half without consulting the
conversation that produced it.

## Repo

`C:\Users\<you>\Repos\OpenWhisper` (or wherever your local clone lives)

Single-shell repo: one Tauri app under `apps/tauri/`, Rust core under `core/`,
new CLI binary under `cli/`. Tasks tracked via Backlog.md CLI
(`backlog task list`), not GitHub Issues.

## Starting state (verify before doing anything)

- Last release tag: `v0.5.0` (2026-05-03). Commits between v0.5.0 and the
  v0.6.0 tag span TASK-62 (PRs #18 + #22), TASK-78 (PR #21), TASK-81 (PR #17),
  TASK-90 (PR #15, formerly TASK-82 — renamed to resolve ID collision with the
  m-1 PR-gate CI task), TASK-87/88/89 (PR #16), and a few small follow-ups.
- Versions in `apps/tauri/package.json` + `apps/tauri/src-tauri/tauri.conf.json`
  + `apps/tauri/src-tauri/Cargo.toml` + `core/Cargo.toml` all bumped to `0.6.0`
  in the version-bump commit on `main` ahead of tagging. `Cargo.lock` carries
  the bumped versions too.
- The Mac side has already built + signed + notarized + smoked the DMG, tagged
  `v0.6.0`, pushed the tag, and created a **draft** GitHub Release with the
  DMG attached. Your job is to attach the MSI + NSIS exe and (after the user
  OKs it) flip draft → published.

## What's in 0.6.0 (scope summary)

User-visible changes since v0.5.0, grouped by area:

- **Crash inspector** (TASK-78) — Rust panic hook writes redacted JSON crash
  files; new Diagnostics → Crashes pane (overview card + full list +
  detail sheet) with Copy backtrace, Open crash folder, and a primary Report
  on GitHub button that pre-fills an issue with the redacted markdown. Sidebar
  rail dot tracks unread crashes. CLI parity via `openwhisper crash-dump <id>`.
- **Diagnostics → Memory pane** (TASK-62 foundation) — live RSS sparkline
  with fixed Y-ceiling + cubic-Bezier interpolation; system memory readout;
  total OW process memory.
- **Diagnostics RSS Breakdown bar** (TASK-62 Stream B) — single platform-aware
  breakdown bar. On Windows the Parakeet segment renders inline (~612 MB
  in-process); on Mac the Parakeet segment is dropped by design because ANE
  weights don't show up in host RSS.
- **Settings → Models — budget bar + storage panel** (TASK-62 Stream B) —
  memory budget bar with hover-ghost preview, per-row delta chips, and a
  storage panel with platform-conditional reveal (Show in Explorer on
  Windows).
- **Keep models warm** (TASK-62 foundation) — new General-pane toggle; when
  off, ModelHandle's idle timer auto-releases the recognizer after a
  configured deadline.
- **Home stats strip** (TASK-87/88/89) — Dictations today / week / all time +
  Time saved, with a Reset stats action and a Typing-speed input under
  Settings → General. SQLite-backed.
- **AppleScript Automation TCC surfacing** (TASK-90, Mac-only — relevant to
  Win only insofar as it confirms Mac parity).
- **`openwhisper` CLI** (TASK-81) — cross-platform binary mirroring the
  headless library surface. Built as a sibling target; the MSI does not bundle
  it on Windows in this release.

## Step 1 — Pull the tag

```sh
git fetch --tags
git checkout v0.6.0
```

Confirm the four manifests show `0.6.0` (`apps/tauri/package.json`,
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

- `target\release\bundle\msi\OpenWhisper_0.6.0_x64_en-US.msi`
- `target\release\bundle\nsis\OpenWhisper_0.6.0_x64-setup.exe`

Cross-compiling Win from Mac does NOT work (wry needs MSVC + WebView2 SDK, MSI
bundling uses WiX). The vendor-natives prereq is automatic — `tauri.conf.json`
chains `pnpm vendor:natives` before the build to copy WebView2Loader.dll +
onnxruntime.dll next to the exe (see `openwhisper-dev-workflow` skill for why).

## Step 3 — Smoke the install

Install the MSI on the build box. Verify against this checklist before
uploading:

1. App launches; mic icon appears in tray; main window follows. No Windows
   OS chrome — Slack-style continuous dark sidebar + titlebar (carried from
   0.5.0).
2. Hotkey starts recording; pill shows; transcribes; injects to focused field.
   Try Notepad, Edge, an Electron app (VS Code or Slack), and Chrome.
3. **Home stats strip** (new) — KV cards show Dictations today / This week /
   All time / Time saved. Run a dictation; today + this week + all-time
   counters bump within ~1 s of completion. Hover Time saved subcaption,
   change Settings → General → Typing speed, return to Home — subcaption
   updates immediately. Hit Reset stats — confirm dialog → counters return
   to 0.
4. **Diagnostics → Memory** (new) — sparkline animates with continuous flow;
   system memory + total OW memory readouts populate; the RSS Breakdown bar
   renders one bar with the Parakeet segment included (Win is in-process,
   ~612 MB once a model loads). Toggle Settings → General → Keep models
   warm OFF; wait the idle window; the Loaded segment drops out of the bar
   and the recognizer unloads.
5. **Diagnostics → Crashes** (new) — open Diagnostics; Crashes card visible
   on the overview. Trigger a panic in dev only (release build won't expose
   the trigger; if you need to repro on release, follow the `dev-panic`
   feature flag note in `apps/tauri/src-tauri/Cargo.toml`). On the crash
   list, open a row → detail sheet renders the redacted backtrace; click
   **Copy backtrace** (icon morphs to CircleCheck briefly), **Open crash
   folder** (Explorer opens to `%APPDATA%\OpenWhisper\crashes\`), and
   **Report on GitHub** (browser opens to a pre-filled issue). Sidebar rail
   dot clears as items are marked read.
6. **Settings → Models** (new) — budget bar at the top of the pane shows a
   physical-memory readout. Hover a disabled row → ghost segment + add chip
   + amber headroom hint. Hover an enabled row → marked departing segment +
   remove chip + green headroom hint. Storage panel: count + path + Show in
   Explorer; Show in Explorer opens the right folder.
7. Esc cancels recording without bleeding to the focused app.
8. **Audio ducking** (carried from 0.5.0): start music in Spotify / browser /
   WMP; trigger hotkey; playback ducks/pauses; release hotkey → playback
   resumes. Tune the BT resume-delay slider if needed.
9. **Launch at login** (carried): toggle the Switch in Settings → General;
   reboot (or sign out / in); confirm the app auto-starts hidden in the tray.
10. Right-click tray → Preferences opens Settings; rebind a hotkey; new
    accelerator appears in the tray "Preferences…" label.
11. Settings → Audio: change input device; live meter responds when toggle on;
    pull mic, app falls back without crashing.
12. Theme picker: System / Light / Dark applies immediately, persists across
    relaunch, no FOUC on boot.

## Step 4 — Upload + (await) publish

```cmd
gh release upload v0.6.0 ^
    target\release\bundle\msi\OpenWhisper_0.6.0_x64_en-US.msi ^
    target\release\bundle\nsis\OpenWhisper_0.6.0_x64-setup.exe
```

**Do NOT publish.** Leave the release in draft until the user explicitly says
to go public. The publish command, once they OK it:

```cmd
gh release edit v0.6.0 --draft=false
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
- **Crash inspector**: TASK-78.5 (launch-toast + bulk-delete) and TASK-78.7
  (Playwright redaction regression) are knowingly deferred — don't treat
  their absence as a release blocker.
- **TASK-62 Stream B Win-side**: TASK-62.11 / .12 / .13 carry Windows code-side
  validation notes in the backlog. The Mac smoke covered the design; the Win
  smoke is the cross-platform check those notes call for.

## Report back

- Confirm the smoke checklist (step 3) is all green.
- Link to the draft GitHub release showing both DMG + MSI + NSIS exe attached.
- Anything unexpected from the build — vendor-natives churn, WiX warnings,
  smoke regressions — call out before the publish step.
