# Pill follows active screen — implementation plan

**Backlog parent:** TASK-55
**Spec:** `docs/superpowers/specs/2026-04-29-pill-follows-active-screen.md`
**Date:** 2026-04-29

Each `### Task N:` heading maps 1:1 to a Backlog subtask `TASK-55.N`. Task ordering matters: 1 first (settings + atomic flag is a dependency for the watcher gate), 2 and 3 can run in parallel, 4 depends on 2+3, 5 depends on 4, 6 depends on 1, 7 last.

---

### Task 1: Settings schema, atomic flag, and commands

**Goal.** Add the `pill.follow_active_screen` block to `settings.json`, expose a process-global `AtomicBool` the watcher can read, and ship the two Tauri commands the UI will call.

**Files.** `apps/tauri/src-tauri/src/settings/mod.rs`, `apps/tauri/src-tauri/src/lib.rs` (handler list).

**Steps.**

1. In `settings/mod.rs`, add:
   ```rust
   #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
   pub struct PillSettings {
       pub follow_active_screen: bool,
   }
   impl Default for PillSettings {
       fn default() -> Self { Self { follow_active_screen: true } }
   }
   ```
2. Extend `SettingsFile` with `#[serde(default)] pill: Option<PillSettings>`.
3. Add a process-global flag and accessor:
   ```rust
   static FOLLOW_ACTIVE_SCREEN: AtomicBool = AtomicBool::new(true);
   pub fn follow_active_screen() -> bool {
       FOLLOW_ACTIVE_SCREEN.load(Ordering::Relaxed)
   }
   ```
4. In `load_settings`, after the file is parsed, set `FOLLOW_ACTIVE_SCREEN` from `parsed.pill.unwrap_or_default().follow_active_screen`.
5. Add `#[tauri::command] pub fn settings_get_pill(app) -> PillSettings` and `#[tauri::command] pub fn settings_set_pill_follow(app, follow: bool) -> Result<(), String>`. The setter must (a) write the JSON file (preserving sibling blocks), (b) flip the atomic.
6. Register both commands in `lib.rs`'s `invoke_handler!` list.
7. `cargo check -p openwhisper-tauri` from `apps/tauri/src-tauri/`. Compile clean.
8. Manual smoke: launch dev build, inspect `settings.json` — `pill.follow_active_screen: true` after first command call.

**Outcome ACs (Backlog).**

- `PillSettings` type added with `follow_active_screen: bool`, default `true`.
- `follow_active_screen()` returns `true` on a fresh checkout (no settings file) and on a settings file without the `pill` block.
- `settings_set_pill_follow(false)` persists `"follow_active_screen": false` to disk and flips the in-memory atomic on the same call.
- Both Tauri commands registered in `invoke_handler!`.
- `cargo check` clean, no new warnings.

---

### Task 2: `focused_window_monitor()` on macOS

**Goal.** Extend the existing AX walk in `fullscreen/mac.rs` to return the origin tuple of the monitor containing the focused window's center, using the thread-safe Core Graphics display API.

**Files.** `apps/tauri/src-tauri/src/fullscreen/mac.rs`, `apps/tauri/src-tauri/Cargo.toml` (if `core-graphics` not already a dep).

**Steps.**

1. Confirm whether `core-graphics` (or `core-graphics-types`) is already in `Cargo.toml` for the Tauri shell. Add if missing.
2. Add FFI bindings for `CGGetActiveDisplayList` and `CGDisplayBounds` (or use the `core-graphics` crate's safe wrappers if available).
3. Extend the AX walk: after `AXFocusedWindow`, fetch `kAXPositionAttribute` (CFTypeRef wrapping `CGPoint`) and `kAXSizeAttribute` (CFTypeRef wrapping `CGSize`). Use `AXValueGetValue` to extract the structs.
4. Compute `(cx, cy)` = window center.
5. Enumerate displays via `CGGetActiveDisplayList`; for each `CGDirectDisplayID`, call `CGDisplayBounds`. The first whose bounds contain `(cx, cy)` wins.
6. Return `Some((bounds.origin.x as i32, bounds.origin.y as i32))`. Return `None` if any step returns null/error/no-match.
7. Keep `is_fullscreen_now()` API unchanged.
8. `cargo check --target aarch64-apple-darwin -p openwhisper-tauri` clean.
9. Manual smoke: temporarily print the result inside the watcher's poll tick on a multi-monitor mac. Focus an app on each display; confirm the printed origin tuple changes. Remove the print before commit.

**Outcome ACs (Backlog).**

- `pub fn focused_window_monitor() -> Option<(i32, i32)>` exists in `fullscreen/mac.rs`.
- Returns `None` when `AXFocusedApplication`, `AXFocusedWindow`, position, or size queries fail.
- Returns `Some(origin)` matching the focused window's display when AX is granted and a window has focus.
- `is_fullscreen_now()` behavior unchanged.
- `cargo check` clean on aarch64-apple-darwin.

---

### Task 3: `focused_window_monitor()` on Windows

**Goal.** Add the same function to `fullscreen/windows.rs`, refactoring the foreground/skip-list/monitor query into a shared helper so `is_fullscreen_now` and the new function don't duplicate.

**Files.** `apps/tauri/src-tauri/src/fullscreen/windows.rs`.

**Steps.**

1. Extract the existing block in `is_fullscreen_now` that does `GetForegroundWindow → pid filter → SHELL_CLASSES filter → GetWindowRect → MonitorFromWindow → GetMonitorInfoW` into a private helper:
   ```rust
   fn foreground_monitor_info() -> Option<(RECT, MONITORINFO)> { ... }
   ```
   Returns `Some((win_rect, mi))` only when none of the skip conditions fire.
2. Refactor `is_fullscreen_now()` to call the helper and apply the `win_rect ⊇ mi.rcMonitor` test on the result. Behavior must be byte-identical to today.
3. Add:
   ```rust
   pub fn focused_window_monitor() -> Option<(i32, i32)> {
       foreground_monitor_info().map(|(_, mi)| (mi.rcMonitor.left, mi.rcMonitor.top))
   }
   ```
4. `cargo check --target x86_64-pc-windows-msvc -p openwhisper-tauri` clean (or run on a Windows host).

**Outcome ACs (Backlog).**

- Private helper `foreground_monitor_info()` extracted; both functions use it.
- `is_fullscreen_now()` returns the same value as before for the same inputs (verify by inspection — no test exists today).
- `focused_window_monitor()` returns `Some((left, top))` from `MONITORINFO.rcMonitor`.
- All four shell-window classes still filtered.
- `cargo check` clean on x86_64-pc-windows-msvc.

---

### Task 4: Watcher emits monitor-changed signal

**Goal.** Extend `fullscreen/mod.rs` so a single 500 ms poll thread serves both the existing fullscreen callback and a new pill-follow callback. Gate the new callback on `settings::follow_active_screen()`.

**Files.** `apps/tauri/src-tauri/src/fullscreen/mod.rs`.

**Steps.**

1. Refactor the singleton: add a second `OnceLock<Callback>` for the monitor signal. Rename existing `install` → `install_fullscreen` (keep an alias `pub use install_fullscreen as install` to avoid touching call sites in this task).
2. Add:
   ```rust
   type MonitorCallback = Box<dyn Fn(Option<(i32, i32)>) + Send + Sync + 'static>;
   static MONITOR_CB: OnceLock<MonitorCallback> = OnceLock::new();
   pub fn install_pill_follow<F>(on_change: F)
       where F: Fn(Option<(i32, i32)>) + Send + Sync + 'static
   { let _ = MONITOR_CB.set(Box::new(on_change)); ensure_poller_started(); }
   ```
3. Replace `INSTALLED.compare_exchange` body with `ensure_poller_started()` so both `install_fullscreen` and `install_pill_follow` route through it. First caller wins; subsequent callers no-op the spawn but still register their callback. Keep `pub use install_fullscreen as install` as a **deliberate, temporary alias** so this task doesn't touch the existing `lib.rs` call site — Task 5 removes the alias once the new wiring lands.
4. Add `static LAST_MONITOR: Mutex<Option<(i32, i32)>> = Mutex::new(None);`.
5. Inside the poller tick, after the existing fullscreen detection:
   ```rust
   if crate::settings::follow_active_screen() {
       if let Some(cur) = focused_window_monitor() {
           let mut last = LAST_MONITOR.lock().unwrap();
           if *last != Some(cur) {
               *last = Some(cur);
               if let Some(cb) = MONITOR_CB.get() { cb(Some(cur)); }
           }
       }
   }
   ```
   (`focused_window_monitor` is the platform-conditional dispatch added earlier — wire `mac::focused_window_monitor` / `windows::focused_window_monitor` via `#[cfg]` blocks alongside the existing `detect_now`.)
6. When the toggle flips OFF mid-session, the gate above suppresses subsequent callbacks but does NOT reset `LAST_MONITOR` — so flipping back ON only fires when the user genuinely changes monitor.
7. `cargo check` clean on both targets.
8. Manual smoke: log the callback invocation (temp `eprintln!`) on a multi-monitor mac. Confirm exactly one fire per real focus-change-across-monitors.

**Outcome ACs (Backlog).**

- One poll thread regardless of which `install_*` function(s) the caller invokes.
- `install_pill_follow` callback fires on monitor-origin change with `Some(origin)`.
- Callback does NOT fire when `focused_window_monitor()` returns `None` (no fallback to primary).
- Callback does NOT fire when `settings::follow_active_screen()` is `false`.
- Existing fullscreen callback behavior unchanged.

---

### Task 5: Reposition pill on monitor change + boot wiring

**Goal.** Rename and extend the existing position command so it accepts an optional monitor-origin hint, then register the watcher callback in `setup()`.

**Files.** `apps/tauri/src-tauri/src/lib.rs`, `apps/tauri/src/PillOverlay.tsx`.

**Steps.**

1. Rename `position_pill_bottom_center` → `reposition_pill` in `lib.rs`. Update the entry in `invoke_handler!` and the call site in `PillOverlay.tsx:201`.
2. **Coordinate-space contract.** The origin tuple is opaque between layers: the watcher emits whatever `focused_window_monitor()` returns (mac: logical points from `CGDisplayBounds.origin`; win: physical px from `MONITORINFO.rcMonitor`), and the lookup is platform-conditional and converts to match. Do **not** try to unify spaces inside `reposition_pill`.
3. Add a platform-conditional helper `find_tauri_monitor(app, origin: (i32, i32)) -> Option<tauri::Monitor>` placed alongside `focused_window_monitor`:
   - **mac** (`fullscreen/mac.rs`): walk `app.available_monitors()`. For each `m`, compute `(m.position().x as f64 / m.scale_factor(), m.position().y as f64 / m.scale_factor())` and round to `i32`. Match against `origin`. (Tauri's `Monitor::position()` is physical px; `CGDisplayBounds.origin` is logical points; converting the Tauri side keeps the watcher tuple as the source of truth.)
   - **win** (`fullscreen/windows.rs`): walk `app.available_monitors()` and compare `m.position()` directly (both watcher tuple and Tauri monitor are physical px in virtual-screen space — no conversion).
4. Add an optional `monitor_origin: Option<(i32, i32)>` parameter to `reposition_pill`. When `Some`, dispatch to `find_tauri_monitor`; on no-match, fall back to `pill.current_monitor()` so a stale tuple (e.g. display unplugged between watcher tick and command dispatch) still places the pill somewhere visible. When `None`, use `pill.current_monitor()` as today.
5. **Wrap the body in `app.run_on_main_thread(move || { ... })`.** This is load-bearing: per the spec's "Risks" section, Tauri's `available_monitors()` may go through `NSScreen.screens` internally on macOS, which is main-thread-only. Do NOT remove the wrapper as "unnecessary" during refactor — the spec explicitly calls this out.
6. In `setup()`, after `fullscreen::install(...)`, remove the `pub use install_fullscreen as install` alias from Task 4 and update the existing call to `fullscreen::install_fullscreen(...)`. Then register the new pill-follow callback:
   ```rust
   let app_for_pill = app.handle().clone();
   fullscreen::install_pill_follow(move |origin| {
       let app = app_for_pill.clone();
       let _ = app.run_on_main_thread(move || {
           // call the same internal placement helper used by reposition_pill
       });
   });
   ```
7. Call `reposition_pill` once in `setup()` after the watcher install — replaces the boot-time placement that `PillOverlay.tsx`'s mount effect currently triggers (or keep the mount-effect call; either is fine but pick one).
8. `cargo check` clean on both targets.
9. Manual smoke (multi-monitor mac dev build):
   - Focus app on display 1 → pill on display 1.
   - Focus app on display 2 → pill jumps to display 2 within ~500 ms.
   - Start recording while focused on display 1 → focus app on display 2 mid-recording → pill follows; level meter continues without flicker; transcript injects into the focused app on display 2 (existing injection path).
10. Single-monitor smoke: log shows zero callback fires over a 30 s session of normal app switching.

**Outcome ACs (Backlog).**

- Pill repositions to bottom-center of new monitor within ~500 ms of foreground switch.
- Recording-in-progress pill follows without disrupting the SVG tween or level meter.
- Single-monitor: zero `set_position` calls beyond the boot placement.
- `reposition_pill` registered in `invoke_handler!`; front-end invoke updated.
- Old `position_pill_bottom_center` symbol removed.

---

### Task 6: Settings UI toggle in General pane

**Goal.** Surface "Follow active screen" in Settings → General, default ON, persisting via the commands from Task 1.

**Files.** `apps/tauri/src/components/general-pane.tsx` (created by TASK-56 — the General pane scaffold lands as `GeneralPane` using shadcn primitives. **Depends on TASK-56.** Do NOT create the pane in this task; only add a row.)

**Steps.**

1. The General pane already exists post-TASK-56 with three sections: Startup / Appearance / About. Add a fourth section **Pill** above About (or merge into Appearance, executor's call) containing a single shadcn `Field` row.
2. Use shadcn primitives consistent with the rest of GeneralPane: `Field` (orientation horizontal) + `FieldLabel` + `FieldDescription` + `Switch`. Do not introduce custom toggle markup.
3. On pane mount: `invoke<PillSettings>('settings_get_pill')` → set local toggle state. Default to `true` if the call rejects (matches Rust-side default) so the UI doesn't hang on first run.
4. On toggle flip: optimistic UI updates state immediately, then `invoke('settings_set_pill_follow', { follow: nextValue })`. On rejection, revert and surface a console warn (consistent with `set_pill_click_through` in `PillOverlay.tsx:196`).
5. Label and helper text:
   - **Label:** "Follow active screen"
   - **Helper:** "Pill jumps to whichever screen has the focused app."

**Outcome ACs (Backlog).**

- General pane renders the toggle and reflects persisted state on open.
- Flipping the toggle updates `settings.json` and the in-process atomic on the same interaction (no restart needed for the watcher to honor the new value).
- New users see toggle in the ON position by default.
- Visual treatment matches the existing Audio pane mic-test toggle.

---

### Task 7: Playwright spec for the toggle

**Goal.** Cover the UI half with a Playwright test. Multi-monitor follow behavior itself is not CI-testable; document manual smoke steps in the spec file.

**Files.** Extend `apps/tauri/tests/settings-window.spec.ts`. The shared harness lives at `apps/tauri/tests/fixtures/tauri-shim.ts` — it exposes `__owEmit` plus `invoke` mocking already used by the Shortcuts and Audio specs (see `settings-window.spec.ts:1` import + `settings-window.spec.ts:193` for an existing "asserts invoke was called with X" pattern).

**Steps.**

1. Add a test using the existing `openSettings(page)` helper that navigates to the General tab and verifies the "Follow active screen" toggle is rendered with checked state on default mount (the shim's default mocked `settings_get_pill` returns `{ follow_active_screen: true }`).
2. Add a test that clicks the toggle and asserts the shim recorded an `invoke` to `settings_set_pill_follow` with `{ follow: false }`. Use the same `mockInvoke` / `getInvokeCalls` (or equivalent) helper used by the existing reset-hotkey assertion at `settings-window.spec.ts:193`. If no `getInvokeCalls`-equivalent exists in the shim today, the smaller fallback AC is "toggle UI state flips visually on click" — pick the stronger assertion only if the shim already supports it.
3. Add a test that, with the shim mocked to return `{ follow_active_screen: false }` from `settings_get_pill`, opens Settings → General and asserts the toggle is rendered in the OFF position.
4. Document the manual multi-monitor smoke steps at the top of the new test block (a `// Manual:` comment). CI cannot verify the actual follow behavior; the spec covers the toggle UI half.
5. `pnpm test:ui` green locally. If browsers missing: `pnpm exec playwright install chromium` then re-run.

**Outcome ACs (Backlog).**

- Three Playwright assertions: default-ON, set-on-flip, hydrate-from-stored-value.
- Manual multi-monitor smoke steps documented in the spec or sibling file.
- `pnpm test:ui` runs green locally and on CI.

---

## Reviewer loop

Once all 7 plan tasks have matching Backlog subtasks, dispatch the plan-document-reviewer agent with the standard plan-review criteria PLUS the verbatim Backlog-enforcement fragment from `.claude/skills/writing-backlog-plans/references/plan-reviewer-addendum.md`. Address findings before handing the plan to an executor.

## Execution handoff

The executor (subagent-driven-development or executing-plans) picks up `TASK-55.1` first, then `.2` and `.3` in parallel, then `.4`, `.5`, `.6` (parallel with `.5`), then `.7`. Status updates flow through `backlog task edit` per the cheatsheet.
