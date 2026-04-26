# Tauri Phase 6 — handover prompt

Paste this to a fresh Claude session (or hand it to a collaborator). Self-contained: a reader cold to the project should be able to start from here, with `docs/tauri-port-handover.md` as backing context for anything not covered.

---

## What ships

From `docs/tauri-port-handover.md` §5 deliverables 9 + 10 + 11 + 12, plus §9 Phase 6:

1. **Close-to-tray** — main window hides on close, app stays alive, only the tray-menu **Quit** terminates. Mirrors the macOS SwiftUI app's behavior (`LSUIElement = true` + `applicationShouldTerminateAfterLastWindowClosed = false`).
2. **Single-instance enforcement** — second launch focuses the existing window instead of starting a parallel process. Use `tauri-plugin-single-instance`.
3. **Health banner polish** — banner itself already shipped in Phase 4 (`HealthBanner` component + `hotkey_status` event + the now-renamed **Restart** button). Phase 6 verifies edge cases: mic-denial banner, recognizer-load-failure banner, Win Ctrl+Space chord conflict banner.
4. **Auto-update** — `tauri-plugin-updater` wired with a placeholder endpoint. WinUI 3 had no auto-update; cheap to wire now even without a release infra yet.

Out of scope (Phase 7 polish): side-by-side visual compare with `apps/macos/`, fullscreen verification across game/video/presentation apps, multi-monitor + RDP sanity, archiving and deleting `apps/windows/`.

## What's already in place

Status of `main` after Phase 5 (commits `f9f58e4..7f809e0`) plus subsequent post-Phase-5 work (`a028208..HEAD`):

- **Phase 5 complete**: text injection (clipboard save → set → Cmd+V/Ctrl+V → restore + 200 ms), fullscreen-aware hotkey teardown + pill hide, proactive mic prompt at boot gated on `hotkey_status_current().ok`, **Restart** button (full app relaunch on macOS, re-attempts install on Windows).
- **Recognizer warmup at boot** — `spawn_recognizer_warmup()` in `apps/tauri/src-tauri/src/lib.rs:134` kicks model load on a background thread on app start so the first Record click hits steady-state latency. Already wired.
- **Tray icon + context menu** — `tray::install` (Phase 4). Includes an Open and a Quit item already; Quit currently calls `app.exit(0)`. Close-to-tray will rely on this Quit being the *only* path that exits.
- **Hotkey + Escape hook + watchdog** — Phase 4. Mac CGEventTap, Win Ctrl+Space chord. `hotkey::install` is idempotent and re-runnable.
- **Cross-platform dev wrapper** — `pnpm dev:tauri` works on both Mac (`scripts/dev-run.sh`) and Win (`scripts/dev-run.ps1`).
- **8 Playwright tests green** (`pnpm test:ui`). Banner test already covers `hotkey_status` ok/error transitions and the Restart button selector.

## Read first

1. `docs/tauri-port-handover.md` — §5 (deliverables 9–12), §9 (Phase 6), §10 (constraints).
2. `apps/macos/App/OpenWhisperApp.swift` — the SwiftUI app's lifecycle, including:
   - `applicationShouldTerminateAfterLastWindowClosed` returning false (close-to-tray semantics)
   - `terminatePriorInstances()` — Mac's pre-Tauri single-instance enforcement (note: Tauri's plugin focuses the existing window instead of killing the new one)
   - The tray menu Quit handler
3. `apps/tauri/src-tauri/src/lib.rs` — Tauri setup. The `setup` closure is where the single-instance plugin and updater plugin will register, and where window close-event handlers attach.
4. `apps/tauri/src-tauri/src/tray.rs` — current tray icon + Quit menu wiring.
5. `apps/tauri/src/components/health-banner.tsx` + `apps/tauri/src/lib/use-hotkey-status.ts` — the banner the user already sees on hotkey failure. Mic-denial / recognizer-load-failure routing into this is a Phase 6 task.
6. `apps/tauri/tests/main-window.spec.ts:123` — the existing banner regression test. Add new cases here for additional banner sources.

## Suggested module layout

Phase 6 is mostly plumbing — most lives in `lib.rs` and `tray.rs`. New surface:

```
src-tauri/src/
  lib.rs           setup() registers single_instance + updater plugins,
                   attaches close-to-tray window-event handler
  tray.rs          existing — confirm Quit is the canonical exit
  updater/         (NEW, optional) wrapper around tauri-plugin-updater
                   with our placeholder endpoint + check-on-boot logic
src/
  components/
    health-banner.tsx       existing — extend if a second banner source needs distinct copy
    update-available.tsx    (NEW) banner / toast when updater finds a new version
  lib/
    use-update-status.ts    (NEW) hook that subscribes to updater events
```

If the updater banner overlaps in look with the health banner, you can reuse `HealthBanner` with a different `retryLabel="Install update"`. Decide based on visual weight — auto-update isn't an error, so likely a separate component.

## Constraints + traps

- **macOS LSUIElement=true** is already set in `Info.plist`. Window close on the last window will quit the NSApp by default unless you override — Tauri 2 exposes this via `app.set_activation_policy(ActivationPolicy::Accessory)` (already implicit via LSUIElement) and a window event listener that `prevent_close()`s. See `project_swiftui_window_lsuielement` memory for the SwiftUI-side gotcha that bit us before — Tauri inherits the same OS-level behavior.
- **`app.exit(0)` from the tray Quit must NOT be replaced with `window.close()`** — close is intercepted to hide. Tray Quit is the only true-exit path.
- **Single-instance plugin needs setup BEFORE the runtime starts** — register on the `Builder`, not in `setup`. The plugin's callback fires in the *original* process when a second launch is attempted; use it to `app.get_webview_window("main").set_focus()`.
- **Updater plugin requires a public-key + endpoint config**. For Phase 6 use a placeholder URL (e.g. `https://updates.openwhisper.invalid/{{target}}/{{current_version}}`) and a generated keypair. Don't ship the private key. Endpoint can 404 — the plugin handles it gracefully.
- **No new TCC surface**. Close-to-tray + single-instance + updater don't trigger any new system permission prompts.
- ~~**Cross-compile gate**: `cargo check --target x86_64-pc-windows-gnu` from Mac (mingw-w64 installed). Verify on each commit.~~ **OUTDATED post-TASK-40**: the ort engine swap dropped sherpa-onnx (which shipped gnu prebuilts) for `ort-sys`, which has no `x86_64-pc-windows-gnu` prebuilts. Build Win on Win, Mac on Mac. Revisit `ort` `load-dynamic` feature at release-packaging time, not for the dev-loop gate.
- **Test loop**: `pnpm test:ui` is the regression gate. Add Playwright cases for:
  - Banner appears for mic-denied (mock the `permissions_status` event if you add one)
  - Update-available banner renders when updater event fires (mock event)
  - Single-instance / close-to-tray are integration concerns — verify manually per OS, not in Playwright (Chromium can't simulate window-close-from-OS).
- **Don't delete `apps/windows/`** in this phase. Phase 7 owns the archival.

## Memories worth checking

- `project_swiftui_window_lsuielement` — last-window-close terminating NSApp is the gotcha. Tauri Mac path needs explicit prevent_close.
- `feedback_rust_core_orchestration` — close-to-tray + single-instance state belongs in Rust, not React.
- `feedback_tauri_ui_test_loop` — `pnpm test:ui` before reporting done; `pnpm pw:open` for ad-hoc probes.
- `project_tcc_dev_pain` — dev-run.sh resets TCC every cycle. Single-instance won't survive across `dev-run.sh` invocations because the binary is force-killed at the start.
- `feedback_zero_config_ux` — auto-update should be on-by-default with no setting, just a "Restart to install" prompt when ready. No setting toggle in MVP.

## Suggested commit order

1. **Single-instance plugin** — smallest change. Register `tauri-plugin-single-instance` in the Builder, callback focuses main window. Verify via `open` twice in quick succession on Mac, double-launch on Win.
2. **Close-to-tray (Mac)** — window event listener: on close-requested, `prevent_close` + `hide`. Confirm tray Quit still exits cleanly.
3. **Close-to-tray (Win)** — same pattern, verify per-OS quirks (Win sometimes fires close on system shutdown — let it through if a shutdown signal accompanies it; otherwise hide).
4. **Updater plugin scaffolding** — register `tauri-plugin-updater` with placeholder endpoint. No UI yet. Verify the plugin probes the endpoint at boot (404 is fine).
5. **Update-available UI** — React component + hook subscribing to updater events. "Install" button calls into the plugin's `Update::download_and_install` flow.
6. **Banner polish** — verify mic-denied, recognizer-load-failure all surface a banner. Wire any missing paths through a unified status event (or keep `hotkey_status` pattern per-concern, decide based on how many concerns there end up being).
7. **Playwright cases** — add tests for new banners. Bump test count from 8 to 10–12.

## First steps for the session

1. Read the files in **Read first** above.
2. Confirm with the user:
   - Updater endpoint placeholder URL — `updates.openwhisper.invalid/{{target}}/{{current_version}}`?
   - Updater key — generate a fresh keypair via `pnpm tauri signer generate`, commit the public key, leave private key out of the repo.
   - Banner consolidation — single component for hotkey + mic + update, or separate?
3. Don't implement until the user confirms the plugin choices.
4. Direct-to-main commits, `Tauri Phase 6: <subject>` prefix, follow `git log --oneline` style. `pnpm` only.
5. Manual smoke per OS at the end; Playwright for what it can cover.

## Definition of done

- `tauri-plugin-single-instance` registered; second launch focuses existing window on both OSes.
- Closing the main window hides it; app stays alive; tray Quit is the only exit.
- `tauri-plugin-updater` registered with placeholder endpoint; UI shows an update-available banner when the plugin reports one.
- Existing 8 Playwright tests stay green; 2–4 new tests cover the new banner sources.
- Mac native `cargo check` clean. (Win-from-Mac cross-compile no longer supported post-TASK-40 — see Constraints + traps. Build Win on Win.)
- Manual smoke on both OSes confirms close-to-tray and single-instance behavior end-to-end.

Realistic total: **1.5–2 working days**.
