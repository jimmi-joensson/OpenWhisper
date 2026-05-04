---
id: doc-14
title: 'Launch at login — design'
type: spec
created_date: '2026-04-29 00:00'
---

# Launch at login — design

**Backlog parent:** TASK-54
**Date:** 2026-04-29
**Status:** Spec → Plan
**Source design:** `SettingsGeneralBoard` row "Launch at login" (`screens.jsx:346`); tray menu Open at Login row (per shipped Swift / WinUI 3 apps).

## Problem

`apps/tauri/src/components/general-pane.tsx` renders a `Launch at login` Switch backed by `useState(true)` — toggle clicks update React state but don't configure OS autostart. On restart the Switch always reads "on" and nothing in the OS reflects the choice. The spec for TASK-56 deferred wiring to TASK-54 with a placeholder note.

The shipped SwiftUI Mac app and the WinUI 3 Windows app both have working autostart toggles via SMAppService / `HKCU\…\Run` respectively. The Tauri shell has shipped without that surface, so the Tauri "Launch at login" Switch is effectively a regression vs. the older shells until TASK-54 closes.

## Goal

Wire `tauri-plugin-autostart` so toggling the Settings Switch (or the tray's Open at Login row) actually configures OS autostart, with the plugin's `is_enabled()` as the single source of truth — no parallel value cached in our own `settings.json`. State must round-trip on each app boot so the toggle reflects external changes (System Settings → Login Items, Task Manager → Startup).

Plus the cross-platform niceties autostart UX needs to feel right:

- App launched at login boots into the tray quietly, no main-window flash.
- Dev/debug builds skip the plugin entirely so cdhash drift on every rebuild doesn't litter `~/Library/LaunchAgents` or `HKCU\…\Run` with stale entries.

## Non-goals (this spec)

- **macOS Developer ID signing.** OW currently ships ad-hoc-signed without hardened runtime (`feedback_tauri_mac_hardened_runtime` memory). Spec accommodates that — does not gate on a future signing-identity migration.
- **Auto-update infrastructure.** Whether the autostart-launched app checks for updates is part of the future TASK-37 update system, not this task.
- **Launch-at-login UI for the Pill window.** Pill is owned/spawned by the main process; "autostart" only ever means "launch the OW main process, which then shows the tray and parks pill on demand."
- **Theme picker (System/Light/Dark).** Already shipped under TASK-54 AC#4 (commit `518f673`); kept out of this scope.

## Per-platform decisions

### macOS — LaunchAgent, not SMAppService

`tauri-plugin-autostart` defaults to `SMAppService.LoginItem` on macOS 13+. SMAppService requires the bundle to be signed with a stable identity and the helper to live inside `Contents/Library/LoginItems/`. OW's release Mac bundle is **ad-hoc-signed without hardened runtime** (`pnpm release:mac` re-signs adhoc-no-runtime per `feedback_tauri_mac_hardened_runtime`). Ad-hoc bundles can't reliably register through SMAppService; behavior across macOS versions is inconsistent.

LaunchAgent fallback writes a plist to `~/Library/LaunchAgents/com.openwhisper.app.plist` pointing at the running binary path. Works for ad-hoc-signed apps. User-visible in System Settings → Login Items → "Open at Login" (with the OW name + path), and toggling it off there mirrors back through `is_enabled()` on next boot.

**Decision:** configure the plugin to use `MacosLauncher::LaunchAgent` explicitly. Revisit (probably automatically becomes SMAppService) when OW gains a Developer ID and notarization in a future task.

The same cdhash-drift concern that invalidates TCC grants on Debug rebuild (`project_tcc_dev_pain`) makes a dev-mode plist fragile — every rebuild changes the binary path / cdhash, and the plist either points at a stale binary or fails Gatekeeper inspection. Reinforces the dev-gate decision below.

### Windows — registry default

The plugin writes `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\OpenWhisper` with the binary path. No signing requirements; no notarization. User-visible in Task Manager → Startup tab. Dev orphaning is just a stale registry value, not a system-level binding — easy to clean up but still preferable to gate dev.

### Linux

Out of scope. OW Tauri shell hasn't stabilized a Linux build target.

## Cross-platform contract

These apply to both Mac and Windows; they're the difference between "plugin wired" and "feels right":

1. **Dev gate.** `#[cfg(not(debug_assertions))]` around `app.plugin(tauri_plugin_autostart::init(...))`. Setting Switch and tray checkbox stay rendered in dev — disabled, with a hint/tooltip that autostart is release-only.
2. **Single source of truth = `is_enabled()`.** Both UI surfaces query the plugin on render and write through `enable()`/`disable()`. Don't shadow the value in `settings.json`.
3. **Boot sync.** Each app start reads `is_enabled()` and broadcasts it to React (initial Switch state) and the tray (CheckMenuItem state). User may have toggled OW via System Settings/Task Manager since last run — UI must reflect the OS truth.
4. **`--autostarted` arg.** Both platforms can be configured to pass an arg flag on boot-launch (`tauri-plugin-autostart` exposes the option). On parsing the flag, the app skips showing the main window, parks in the tray, and is ready for the user's hotkey.
5. **State change broadcast.** When the Settings Switch toggles, the tray's CheckMenuItem must reflect immediately (and vice-versa). Plugin's `enable/disable` is the write path; React + tray each subscribe to a small `autostart_changed` event the Rust commands emit so UI doesn't drift between surfaces.

## Components used

| Need | Component / API |
|---|---|
| Plugin | `tauri-plugin-autostart = "2"` (Cargo + JS guest bindings) |
| Settings Switch (existing) | `Switch` in `general-pane.tsx` — replace local `useState` with `invoke` round-trip + listen for `autostart_changed` |
| Tray check item | `CheckMenuItemBuilder` (Tauri `tauri::menu::CheckMenuItemBuilder`) inserted into the existing tray menu in `src-tauri/src/tray/mod.rs` |
| Boot-flag detection | `std::env::args` in `setup()`; suppress `main.show()` and `main.set_focus()` when `--autostarted` is present |

## Risks

- **Plugin's macOS LaunchAgent path correctness when OW is launched from different locations** (DMG-mount preview, /Applications install, dev `target/debug`). The plist hardcodes the binary path at register time. Practical mitigation: dev gate prevents the dev-binary case; users running from /Applications get a stable path. Users running from a mounted DMG who toggle on without copying the app first will land a plist pointing at a path that doesn't survive ejecting the DMG. Same UX as Superwhisper / Dropbox have to handle; acceptable until we add a "Move to Applications" prompt (separate task).
- **Tray + Settings race when toggling fast.** Both surfaces' write-through hits the same plugin. Plugin internally serializes; the lossy case is two updates within one frame both broadcasting back, causing one render-cycle of stale state. Minor; React will reconcile to the final `is_enabled()` value.
- **Windows `--autostarted` arg quoting.** The registry value is a single string; plugin handles quoting per its docs but it's a per-platform foot-gun if the install path has spaces. Verify the auto-write at register time produces a parseable arg-vector.
- **Tauri sandbox / capabilities.** `tauri-plugin-autostart` needs the plugin permission in `apps/tauri/src-tauri/capabilities/default.json` (or main.json equivalent). Easy to forget — the plugin will silently no-op on a missing capability with no useful error.

## References

- TASK-54 backlog entry — already lists ACs #1–#4 (#4 done). This spec/plan tightens the implementation contract for #1–#3.
- Plugin docs: https://v2.tauri.app/plugin/autostart/
- Tauri menu API: `tauri::menu::CheckMenuItemBuilder` (used identically to `MenuItemBuilder` plus `.checked(bool)` and `.set_checked()`).
- Existing tray module: `apps/tauri/src-tauri/src/tray/mod.rs`. The check item slots into `build_menu`'s composition between `toggle_item` and `prefs_item`.
- GeneralPane: `apps/tauri/src/components/general-pane.tsx`. The Switch row already exists with its `id`, `htmlFor`, `FieldDescription`, etc.; only the state plumbing changes.
- Memory: `feedback_tauri_mac_hardened_runtime`, `project_tcc_dev_pain` — both inform the dev-gate + LaunchAgent decisions.
