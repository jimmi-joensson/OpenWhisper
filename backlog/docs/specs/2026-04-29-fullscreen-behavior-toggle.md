---
id: doc-12
title: 'Setting: Show in fullscreen apps — design'
type: spec
created_date: '2026-04-29 00:00'
---

# Setting: Show in fullscreen apps — design

**Backlog parent:** TASK-58
**Date:** 2026-04-29
**Status:** Spec → Plan
**Source:** `apps/tauri/src-tauri/src/fullscreen/{mod,mac,windows}.rs` + the gating callback at `lib.rs:581`.

## Problem

OpenWhisper currently auto-deactivates whenever a fullscreen app is detected on the focused screen — pill hides, hotkey is suppressed (`lib.rs:581-585`). This is the right default for games, video players, presentations: typing into / dictating onto a fullscreen game is rarely what the user wants, and surrendering Right-Cmd / Right-Alt to the fullscreen app is the correct behavior.

But it's a hard rule baked into the boot path with no opt-out. Users who want to dictate into a fullscreen YouTube comment box, a fullscreen IDE, a fullscreen browser tab, or a "borderless windowed" game with chat have no way to keep OW active. The TASK-57 fix tightened detection accuracy (chromeless monitors no longer false-positive maximized windows as fullscreen), but the policy stayed binary: detected fullscreen → deactivate.

## Goal

Add one core setting — `behavior.show_in_fullscreen`, default `false` — that lets the user opt out of fullscreen deactivation. When `true`, the detector still runs (so we can attribute behavior to it later), but its callback no-ops on the deactivation surface: pill stays visible, hotkey stays active.

When `false` (default), behavior is identical to today, **plus** one new branch: if a recording is currently in flight when fullscreen is entered, the recording aborts silently — audio buffer dropped, no transcript delivered, no paste. Rationale: pasting transcribed text into a fullscreen game / video is more surprising than losing what the user said. Anyone dictating while opening a game has bigger UX problems.

## Non-goals (this spec)

- **Per-app rules.** No "always-on for app X, off for app Y" allowlist. Single global toggle for now.
- **Pill positioning when overlaying fullscreen on macOS.** TASK-55 (pill follows active screen) handles positioning. This spec only ensures the pill *can* render over fullscreen Spaces (collection-behavior side); it doesn't change how its target screen is chosen.
- **Exclusive-fullscreen DirectX games (Windows).** Some DX games push every other window below the swap-chain regardless of `alwaysOnTop`. Best-effort; spec acknowledges and doesn't try to fight it. UI hint can later note this if it confuses users.
- **Mid-recording with setting=on.** No special handling — recording continues, hotkey works normally for STOP.
- **Tray balloon / pill flash to signal "OpenWhisper paused" when hotkey is pressed in fullscreen with setting=off.** Confirmed silent no-op. Future task can layer feedback if users ask.
- **TASK-55.6's "follow active screen" toggle.** That's its own setting in its own task. This spec doesn't merge or conflict with it; both can land independently in General pane sections.

## Per-platform behavior

### macOS

Detector at `fullscreen/mac.rs` returns `is_fullscreen_now()` based on AX-frontmost window properties (Space type, frame-vs-screen). Already production-correct.

For setting=on to actually work on Mac, the pill window's `NSWindow` collection behavior needs `canJoinAllSpaces` + `fullScreenAuxiliary` so the pill is allowed to draw over fullscreen Spaces (which live in their own Space and would otherwise hide a pill set to a normal level). Tauri 2 exposes this via `WebviewWindow::set_visible_on_all_workspaces(true)` (maps to the AppKit collection-behavior call). `tauri.conf.json` currently has `visibleOnAllWorkspaces: false` for the pill — keep the bootstrap value as-is (no need to spam Spaces when the user hasn't opted in), and flip it dynamically on setting change.

The pill window level may also need bumping above `NSStatusWindowLevel` for fullscreen overlay. Tauri 2's `set_always_on_top(true)` maps to `kCGMaximumWindowLevelKey - 1` on Mac which is generally sufficient. Already set in `tauri.conf.json` (`alwaysOnTop: true`). No change needed.

### Windows

Detector at `fullscreen/windows.rs` is now correct post-TASK-57 (WS_MAXIMIZE escape hatch + chromeless-screen tiebreaker).

For setting=on, the pill window has `alwaysOnTop: true` already (`tauri.conf.json`) which the Tauri runtime maps to `HWND_TOPMOST`. Borderless-fullscreen apps (Chrome F11, video, modern games) honor this: pill draws over them. **Exclusive-fullscreen** DirectX9/10/11 games composite their swap-chain directly to the desktop and can push topmost windows below; this is a Windows-platform reality, not a bug, and not solvable from user-mode. Documented as best-effort.

No `set_visible_on_all_workspaces` analog needed on Windows — virtual desktops are different from Mac Spaces, and the pill defaults to following the active virtual desktop which is what we want.

## Setting plumbing

- **Storage:** core settings (where audio prefs and hotkey bindings live), via the existing `apps/tauri/src-tauri/src/settings.rs` module's pattern. Not localStorage — the Rust-side detector callback at `lib.rs:581` needs synchronous read access without invoking through the WebView.
- **Key:** `behavior.show_in_fullscreen` (or whatever the existing settings JSON layout calls for; the executor may pick a flatter name like `show_in_fullscreen` if the schema is shallow). Single boolean; default `false`.
- **Rust commands:** `behavior_get_show_in_fullscreen() -> bool`, `behavior_set_show_in_fullscreen(enabled: bool) -> Result<(), String>`. Setter persists, then emits `behavior_show_in_fullscreen_changed` with the new boolean payload so other Rust subscribers (and the React UI on the next render) can react.
- **In-process cache:** an `AtomicBool` next to the existing `fullscreen::ACTIVE` flag — let's call it `BYPASS_FULLSCREEN_DEACTIVATION`. Setter writes to it after persistence; the detector callback reads it on every transition. Saves a settings-file read on every fullscreen poll.

## UI surface

Settings → General → new section **Behavior** (between Appearance and Updates), or extend an existing section if the Behavior block doesn't exist yet when the executor lands this. TASK-55.6 may also drop a "Pill" or "Behavior" section first; coordinate by reading current `general-pane.tsx` and either extending the existing structure or creating a new section.

Row contents:

- `FieldLabel` — "Show in fullscreen apps"
- `FieldDescription` — "Keeps the pill visible and the hotkey active even when another app is in fullscreen. Off by default — most users want OpenWhisper to step aside for games and video."
- `Switch` — bound to the setting via a `useShowInFullscreen()` hook that mirrors the `useTheme()` / planned `useAutostart()` shape: `invoke` for read/write, `listen` for cross-surface broadcast.

No tray surface — toggling fullscreen behavior from the menubar is unusual and not requested.

## Risks

- **Detector + setting interaction during recording.** When the detector fires `is_fullscreen=true` and a recording is in flight with setting=off, the callback needs to abort cleanly. The dictation core (`openwhisper-core::dictation`) has a stop/abort path; the abort branch needs to drop audio without emitting a transcript event. If the existing API only has "stop and transcribe", we may need to add an `abort` distinct from `stop` (worth checking during plan). Spec assumes either a path exists or one will be added in the relevant subtask.
- **macOS Spaces flicker on toggle.** Flipping `visibleOnAllWorkspaces` at runtime can cause a brief Space-list animation. Acceptable — toggle is a user-initiated, infrequent action.
- **Windows exclusive-fullscreen reality.** As noted above, DirectX exclusive-fullscreen will still hide the pill regardless of setting=on. Description copy already softens this ("best effort"); don't promise more than the platform delivers.
- **Existing TASK-55.6 "follow active screen" semantics.** When pill follows the active screen and that screen has a fullscreen app: setting=off → pill follows then hides; setting=on → pill follows and stays visible. No new conflict; both behave as their independent ACs prescribe.

## References

- Detector module: `apps/tauri/src-tauri/src/fullscreen/{mod,mac,windows}.rs`
- Current callback wiring: `apps/tauri/src-tauri/src/lib.rs:576-585` (`fullscreen::install` registers the deactivate callback)
- Settings module: `apps/tauri/src-tauri/src/settings.rs` (existing schema + load/save pattern)
- Theme hook (shape model): `apps/tauri/src/lib/use-theme.tsx`
- Planned autostart hook (also shape model): TASK-54.3 plan task
- TASK-57: Windows fullscreen detection accuracy fix (prerequisite for trustworthy detection — already done)
- TASK-55.6: pill follows active screen toggle (sibling Settings UI work)
