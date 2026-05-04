---
id: doc-3
title: 'Launch at login — implementation plan'
type: plan
created_date: '2026-04-29 00:00'
---

# Launch at login — implementation plan

**Backlog parent:** TASK-54
**Spec:** `backlog/docs/specs/2026-04-29-launch-at-login.md`
**Date:** 2026-04-29

Each `### Task N:` heading maps 1:1 to a Backlog subtask `TASK-54.N`. Sequential — Task 1 wires the plugin the rest depend on; Task 5 lands tests against the wired-up Switch.

The pre-existing TASK-54 ACs stay as-is (#1 plugin wired, #2 Settings reads/writes via plugin, #3 tray CheckMenuItem synced, #4 theme stub — already done in `518f673`). This plan's subtasks slice that same scope into executable units; each subtask's ACs verify a specific outcome along the way.

---

### Task 1: Add `tauri-plugin-autostart`, dev gate, capability, Rust commands

**Goal.** Bring the plugin into the Tauri shell, gate it to release builds, expose Rust commands the React app can `invoke`, and emit an `autostart_changed` event so other surfaces can subscribe.

**Files.** `apps/tauri/src-tauri/Cargo.toml`, `apps/tauri/src-tauri/src/lib.rs`, `apps/tauri/src-tauri/capabilities/default.json` (or whichever capability file `lib.rs` registers), `apps/tauri/package.json` (JS plugin guest bindings), `apps/tauri/src-tauri/src/autostart.rs` (new).

**Steps.**

1. From `apps/tauri/src-tauri/`, add the Rust dep:
   ```bash
   cargo add tauri-plugin-autostart
   ```
   Confirm Cargo.lock updates and the version pin matches Tauri 2 (currently `2.x`).
2. From `apps/tauri/`, add the JS guest bindings:
   ```bash
   pnpm add @tauri-apps/plugin-autostart
   ```
3. New `apps/tauri/src-tauri/src/autostart.rs` exposes three commands:
   ```rust
   #[tauri::command]
   pub fn autostart_get(app: AppHandle) -> Result<bool, String> { ... }

   #[tauri::command]
   pub fn autostart_set(app: AppHandle, enabled: bool) -> Result<(), String> {
       // call manager.enable() / disable(); on success emit "autostart_changed" with the new bool.
   }

   #[tauri::command]
   pub fn autostart_supported() -> bool {
       cfg!(not(debug_assertions))
   }
   ```
   The `_supported` command lets the UI know whether to disable the Switch + show the dev hint without the React side duplicating the cfg knowledge.
4. In `lib.rs`'s `run()`, register the plugin only in release builds. Pass the `--autostarted` arg flag and macOS LaunchAgent launcher:
   ```rust
   #[cfg(not(debug_assertions))]
   let builder = builder.plugin(
       tauri_plugin_autostart::init(
           tauri_plugin_autostart::MacosLauncher::LaunchAgent,
           Some(vec!["--autostarted"]),
       ),
   );
   ```
5. Register the three commands in `tauri::generate_handler![…]` alongside the existing handlers.
6. Add the plugin permission to the capability file. Find the existing `permissions: [...]` array (where `core:default`, etc. live) and add `"autostart:default"`. Without this the plugin silently no-ops on `enable()` calls.
7. Verify `cargo check` passes from `apps/tauri/src-tauri/`.

**Outcome ACs (Backlog).**

- `tauri-plugin-autostart` listed in `Cargo.toml`; `@tauri-apps/plugin-autostart` listed in `package.json`.
- Plugin registered with `MacosLauncher::LaunchAgent` and `--autostarted` arg, gated on `#[cfg(not(debug_assertions))]`.
- New `autostart.rs` exports `autostart_get`, `autostart_set`, `autostart_supported`; all three registered in `generate_handler!`.
- Capability file lists `autostart:default`.
- `autostart_set` emits an `autostart_changed` event with the new boolean payload on success.
- `cargo check` clean from `apps/tauri/src-tauri/`.

---

### Task 2: `--autostarted` boot path — start hidden in tray

**Goal.** When the app is launched at login (arg flag present), suppress the main-window show/focus and let the tray be the sole visible surface. Hotkey + dictation flow keeps working unchanged.

**Files.** `apps/tauri/src-tauri/src/lib.rs` (the `setup()` block).

**Steps.**

1. Inside `setup()`, parse `std::env::args` once. Record the boolean `auto_started = args.iter().any(|a| a == "--autostarted")` before the existing main-window setup runs.
2. The existing block at `lib.rs:518` looks up the main window and overrides its title. Leave that. After it, where the close-to-tray handler is wired, conditionally do **not** call any show/unminimize/focus on the main window when `auto_started == true`. The close-handler that hides on close stays unchanged.
3. The main window is currently configured with no `visible: false` in `tauri.conf.json` — confirm whether Tauri shows the window before `setup()` runs. If yes (default behavior), the auto-started branch needs to call `main.hide()` early in `setup()` so we don't get a window flash. If `tauri.conf.json` already declares `visible: false` we skip that. Whichever the current state, the AC is "no main-window flash on auto-start launch."
4. Tray installation (`tray::install`) runs unchanged — the tray is the entry point in this mode.
5. Verify by running the binary manually with the flag set:
   ```bash
   cargo tauri dev -- --autostarted
   ```
   (Dev gate skips the plugin registration, but the boot-path code is independent of the plugin and should still recognize the arg.) Confirm: no main window appears; tray glyph visible; main window can be opened on demand via tray "Open" menu item.

**Outcome ACs (Backlog).**

- `setup()` parses `--autostarted` from `std::env::args` and records the boolean.
- When the flag is present, the main window is not shown / focused on boot; tray + pill remain functional.
- When the flag is absent, the existing boot behavior is unchanged (main window appears as today).
- Manual smoke confirms both boot paths.

---

### Task 3: Hook GeneralPane Switch through plugin

**Goal.** Replace `useState(true)` in `general-pane.tsx` with a controlled value backed by `invoke<bool>("autostart_get")` and `invoke("autostart_set", { enabled })`. Subscribe to `autostart_changed` so toggles from the tray surface here too. Disable + add a hint when `autostart_supported()` returns false (dev builds).

**Files.** `apps/tauri/src/components/general-pane.tsx`, `apps/tauri/src/lib/use-autostart.ts` (new — hook to encapsulate the invoke + listen plumbing).

**Steps.**

1. New `apps/tauri/src/lib/use-autostart.ts` exports a `useAutostart()` hook returning `{ enabled, setEnabled, supported }`. Internally:
   - `useEffect` on mount: `invoke<boolean>("autostart_get")` → state, `invoke<boolean>("autostart_supported")` → state.
   - `useEffect`: `listen<boolean>("autostart_changed", (e) => setEnabledState(e.payload))`. Cleanup removes the listener.
   - `setEnabled = (next: boolean) => invoke("autostart_set", { enabled: next })` — does not update local state directly; the round-trip via `autostart_changed` does.
2. In `general-pane.tsx`, replace the local `launchAtLogin` state with `useAutostart()`. The Startup section becomes:
   ```tsx
   const { enabled: launchAtLogin, setEnabled: setLaunchAtLogin, supported } = useAutostart();
   …
   <Switch
     id="launch-at-login"
     checked={launchAtLogin}
     onCheckedChange={setLaunchAtLogin}
     disabled={!supported}
   />
   ```
   When `!supported`, append a `<FieldDescription>` line: "Available in release builds." The existing description ("OpenWhisper runs in the background…") stays as the primary description.
3. `pnpm tsc --noEmit` clean from `apps/tauri/`.

**Outcome ACs (Backlog).**

- New `use-autostart.ts` hook fronts the Rust plugin via `invoke` + `listen`.
- GeneralPane's Switch state comes from `useAutostart()`; local `useState(true)` is gone.
- Dev builds render the Switch disabled with the "Available in release builds" hint; release builds render it enabled and live.
- `pnpm tsc --noEmit` clean.

---

### Task 4: Tray `CheckMenuItem` — Open at Login row

**Goal.** Add an "Open at Login" check item to the tray right-click menu, between "Toggle Dictation" and "Preferences…". The check reflects `is_enabled()` on every menu rebuild; clicking it calls `autostart_set` which emits `autostart_changed` — consumed by both the tray watcher (rebuilds menu) and the React `useAutostart` hook.

**Files.** `apps/tauri/src-tauri/src/tray/mod.rs`.

**Steps.**

1. In `build_menu`, add an autostart check item between `toggle_item` and `prefs_item`. Use `CheckMenuItemBuilder`:
   ```rust
   use tauri::menu::CheckMenuItemBuilder;
   …
   let autostart_enabled = autostart::is_enabled(app); // helper in autostart.rs
   let autostart_item = CheckMenuItemBuilder::with_id(&ids.autostart, "Open at Login")
       .checked(autostart_enabled)
       .enabled(autostart::supported())
       .build(app)?;
   ```
   Add `autostart: String` to `MenuIds` so the menu-event handler can route the click.
2. In the `on_menu_event` handler (currently dispatching open / toggle / prefs / quit), add a branch for `id == ids.autostart` that calls `autostart::toggle(app)` (a small helper in `autostart.rs` that flips `is_enabled` via the plugin and emits `autostart_changed`).
3. The existing tray phase-watcher thread (`thread::spawn` at `tray/mod.rs:308`) rebuilds the menu on every phase change. Extend this so the watcher also rebuilds when an `autostart_changed` event fires. Easiest: register an event listener via `app.listen("autostart_changed", …)` inside `tray::install`; on event, look up the tray and call `tray.set_menu(Some(rebuilt))`. Reuse `build_menu` — it already reads the current autostart state.
4. Verify with `cargo check`.

**Outcome ACs (Backlog).**

- Tray right-click menu has an "Open at Login" check item between "Toggle Dictation" and "Preferences…".
- Check reflects `is_enabled()` on first show + after every `autostart_changed`.
- Clicking the item flips the plugin state and broadcasts to other surfaces (Settings React Switch reflects within one render).
- In dev builds the item renders disabled.
- `cargo check` clean.

---

### Task 5: Playwright spec for the Settings UI behavior

**Goal.** Cover the React side of the wiring. Mock the `invoke` boundary and the `listen` event, assert the Switch responds correctly. Don't try to test actual OS autostart from Playwright — that's an integration concern outside the WebView's reach.

**Files.** `apps/tauri/tests/settings-window.spec.ts` (extend), possibly `apps/tauri/tests/fixtures/tauri-shim.ts` if a new helper is needed.

**Steps.**

1. Extend the existing tauri shim (or add to it) so tests can stub:
   - `autostart_get` → returns a fake initial value
   - `autostart_set` → records the last-set value to a `window.__owAutostartLastSet`
   - `autostart_supported` → returns a fake boolean
   - emit `autostart_changed` from a helper similar to `emitTick` / `emitDeviceState`.
2. Add to the `test.describe("settings view", …)` block:
   - **"Launch at login Switch reflects autostart_get"**: shim returns `false`, open Settings, assert Switch is not checked.
   - **"Toggling the Switch invokes autostart_set with the new value"**: shim starts `true`, click Switch (or use BaseUI's role=switch + `setChecked`), assert `__owAutostartLastSet === false`.
   - **"autostart_changed event updates the Switch"**: open Settings, emit `autostart_changed: true`, assert Switch becomes checked. (Covers the tray-flip-affects-Settings path.)
   - **"In dev / unsupported builds the Switch is disabled with the hint"**: shim returns `supported=false`, assert `aria-disabled` (or `:disabled` attribute) and the "Available in release builds" hint is visible.
3. Verify the existing Settings tests + the three Theme + Section structure tests from TASK-56.4 still pass (the shim change shouldn't affect them).
4. `pnpm test:ui` green locally. If browsers missing: `pnpm exec playwright install chromium` then re-run.

**Outcome ACs (Backlog).**

- Tauri shim exposes `autostart_get`, `autostart_set`, `autostart_supported` stubs and an `emitAutostartChanged` helper.
- Four new tests assert: initial state from `autostart_get`, write-through via `autostart_set`, external update via `autostart_changed`, dev-build disabled state with hint.
- Existing Settings + General-pane tests still pass.
- `pnpm test:ui` green locally and on CI.

---

## Reviewer loop

After all 5 plan tasks have matching Backlog subtasks (`TASK-54.1` through `TASK-54.5`), dispatch the plan-document-reviewer subagent with the standard plan-review criteria PLUS the verbatim Backlog-enforcement fragment from `.claude/skills/writing-backlog-plans/references/plan-reviewer-addendum.md` AND a Tauri-specific check that the dev-gate is unambiguous (no `#[cfg(debug_assertions)]` polarity slips) and that the capability change is in the right capability file (whichever `lib.rs`'s registered handler points at).

## Execution handoff

Sequential: 1 → 2 → 3 → 4 → 5. No parallelism — each task feeds the next:

- Task 1 lands the Rust commands; Task 3 needs them to invoke against.
- Task 2's boot path is independent of plugin registration but uses the same arg the plugin lands. Slot it before Task 3 so manual smoke of "no flash on dev launch" is easy to verify.
- Task 4 listens for the same `autostart_changed` event Task 1 emits.
- Task 5 stubs against the shapes Tasks 1+3 land.

Status updates flow through `backlog task edit` per the cheatsheet. Each subtask appends commit refs in implementation notes (`--append-notes`), checks ACs as they land (`--check-ac`), and ends with a `--final-summary` + `-s Done`.

## TDD shape note

Tasks 3 and 5 lean TDD: the tests in Task 5 cover the hook from Task 3, and writing them in either order works (test-first preferred per project convention; spec the hook's invoke signatures, then write the implementation, then iterate until tests pass). Tasks 1, 2, and 4 are integration-shaped (Rust crate boundary, Tauri lifecycle, OS plugin) — `cargo check` + manual smoke is the pragmatic verification, with the React-side Playwright covering the visible surface in Task 5.
