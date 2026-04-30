---
id: TASK-60
title: 'Wire Launch-at-login backend (Windows + Mac) — autostart plugin'
status: To Do
assignee: []
created_date: '2026-04-30 09:35'
labels:
  - windows
  - macos
  - settings
  - hotfix
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The "Launch at login" Switch in Settings → General → Startup is a UI stub in v0.4.0 — its `onCheckedChange` toggles a local `useAutoStart` setting in the React layer but no backend ever applies it. Verified on Windows during the v0.4.0 release smoke (2026-04-30): enable the Switch → sign out of Windows → sign back in → OpenWhisper does NOT launch automatically.

Wire the backend so the Switch actually controls autostart. Recommended path: `tauri-plugin-autostart` (officially supported, handles both Windows registry under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` and macOS `~/Library/LaunchAgents/<bundle-id>.plist` with the right key for login-item registration).

This is scoped as a **v0.4.1 hotfix** — does not block v0.4.0. The handover doc already calls the Switch out as a known stub, so the v0.4.0 release notes don't promise it.

Cross-platform parity is part of the AC: even though the regression was only confirmed on Windows, Mac must be tested explicitly. The Mac side has its own pitfalls (LSUIElement apps need a different LaunchAgent layout, login-items API behaviour changed across macOS major versions).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria

<!-- AC:BEGIN -->
- [ ] #1 Add `tauri-plugin-autostart` (or equivalent) to `apps/tauri/src-tauri/Cargo.toml` and register it in the Tauri builder in `apps/tauri/src-tauri/src/main.rs` (or wherever plugins are registered).
- [ ] #2 The Switch in `apps/tauri/src/settings/General/Startup.tsx` (or wherever the existing UI stub lives) calls a Tauri command that enables/disables autostart on flip. Toggling the Switch off removes the autostart entry; toggling it on creates the entry.
- [ ] #3 The Switch state is read from the actual platform autostart registration on mount, not just from local React state — so a user who enabled autostart in a previous run sees the Switch reflect reality after a restart.
- [ ] #4 Manual smoke on Windows 11: enable Switch → close OpenWhisper → sign out of Windows → sign back in → OpenWhisper launches automatically (tray icon visible). Then disable Switch → sign out → sign back in → OpenWhisper does NOT launch.
- [ ] #5 Manual smoke on macOS (Sequoia 15+, arm64): enable Switch → quit OpenWhisper → log out → log in → OpenWhisper launches and the menu-bar mic icon appears (no Dock icon, since LSUIElement = true). Then disable Switch → log out → log in → OpenWhisper does NOT launch.
- [ ] #6 Behaviour with the autostart plugin is documented in a code-comment at the registration site (or a short module doc-comment) covering: which platform-level mechanism is used on each OS, the bundle-identifier the registration is keyed on, and how to manually inspect the registration (Windows: `reg query HKCU\Software\Microsoft\Windows\CurrentVersion\Run`; macOS: `ls ~/Library/LaunchAgents/`).
- [ ] #7 v0.4.1 release notes mention "Launch-at-login Switch is now functional on both Windows and macOS" and the v0.4.1 release ships with both platforms verified.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Background pointers:

- The current Switch is referenced in `tauri.conf.json`-adjacent React code; locate via `grep -r "Launch at login" apps/tauri/src` plus the General pane scaffold landed under TASK-56. The Switch already has wired hover/disabled styling — only its `onCheckedChange` is the stub.
- `tauri-plugin-autostart` exposes `enable`, `disable`, and `is_enabled` commands. The frontend can either call these directly via `invoke` or thread them through an existing Settings command for symmetry with other persisted toggles.
- Watch out for the macOS bundle-identifier conflict noted in `tauri.conf.json` (`com.openwhisper.app` ends with `.app` — the warning Tauri logs at build time). LaunchAgent plists key off the bundle ID; if anyone changes it later, autostart entries will silently break for users who upgrade.
- Do NOT roll a custom registry-write / plist-write — `tauri-plugin-autostart` already handles the cross-platform differences (Windows HKCU Run key vs LaunchAgent plist vs macOS Service Management framework on newer OS versions). Custom code here would re-invent the bugs the plugin already solved.
- LSUIElement = true on Mac means OpenWhisper has no Dock icon. Some autostart plumbing assumes a Dock-icon app; verify the plugin's macOS path uses a LaunchAgent plist (works for menu-bar apps) and not the Service Management framework's "login item" API (which expects a normal app and may fail silently or show a system-managed login-items entry the user doesn't expect).
<!-- SECTION:NOTES:END -->
