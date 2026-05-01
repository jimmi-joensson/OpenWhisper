# Setting: Show in fullscreen apps — implementation plan

**Backlog parent:** TASK-58
**Spec:** `backlog/docs/specs/2026-04-29-fullscreen-behavior-toggle.md`
**Date:** 2026-04-29

Each `### Task N:` heading maps 1:1 to a Backlog subtask `TASK-58.N`. Sequential — Task 1 lands the schema + commands the rest depend on; Task 5 lands tests against the wired-up Switch.

---

### Task 1: Settings schema + Rust commands + in-process cache

**Goal.** Add the `show_in_fullscreen` boolean to the core settings file with a `false` default, expose `behavior_get_show_in_fullscreen` / `behavior_set_show_in_fullscreen` Tauri commands, emit `behavior_show_in_fullscreen_changed` on writes, and keep an `AtomicBool` mirror so the fullscreen detector callback can read it without hitting disk.

**Files.** `apps/tauri/src-tauri/src/settings.rs`, `apps/tauri/src-tauri/src/behavior.rs` (new), `apps/tauri/src-tauri/src/lib.rs` (handler registration + initial cache hydrate).

**Steps.**

1. Read `settings.rs` to understand the existing settings struct's serde shape. Add a new field — recommended `show_in_fullscreen: bool` — with `#[serde(default)]` so older settings files round-trip without breaking, and a `default_show_in_fullscreen() -> bool { false }` if the existing pattern uses field-level defaults.
2. New `apps/tauri/src-tauri/src/behavior.rs` defines:
   ```rust
   use std::sync::atomic::{AtomicBool, Ordering};

   static SHOW_IN_FULLSCREEN: AtomicBool = AtomicBool::new(false);

   pub fn show_in_fullscreen() -> bool {
       SHOW_IN_FULLSCREEN.load(Ordering::Relaxed)
   }

   pub fn set_show_in_fullscreen_cache(value: bool) {
       SHOW_IN_FULLSCREEN.store(value, Ordering::Relaxed);
   }

   #[tauri::command]
   pub fn behavior_get_show_in_fullscreen(...) -> Result<bool, String> { ... }

   #[tauri::command]
   pub fn behavior_set_show_in_fullscreen(app: AppHandle, enabled: bool) -> Result<(), String> {
       // 1. Persist via settings::save_settings(...)
       // 2. set_show_in_fullscreen_cache(enabled)
       // 3. app.emit("behavior_show_in_fullscreen_changed", enabled)
   }
   ```
3. In `lib.rs::run()`, register the commands in `tauri::generate_handler![…]`. After the existing `settings::load_settings(...)` call in `setup()`, call `behavior::set_show_in_fullscreen_cache(loaded.show_in_fullscreen)` so the cache is hot before the fullscreen poll thread starts.
4. `cargo check` clean from `apps/tauri/src-tauri/`.
5. Smoke: write a small test or use `cargo test` (depending on existing test infra) to confirm round-trip persistence — set true, restart, observe true.

**Outcome ACs (Backlog).**

- `settings.rs` schema includes `show_in_fullscreen: bool` with default `false`.
- New `behavior.rs` exposes `show_in_fullscreen()` cache reader, `set_show_in_fullscreen_cache(...)` writer, and the two Tauri commands.
- `behavior_set_show_in_fullscreen` persists, updates cache, and emits `behavior_show_in_fullscreen_changed` with the new boolean.
- Commands registered in `generate_handler!`; cache hydrated in `setup()` from the loaded settings.
- `cargo check` clean.

---

### Task 2: Detector callback honors the setting; abort recording on fullscreen-entry

**Goal.** Update the `fullscreen::install` callback at `lib.rs:581` to read the cache and skip the deactivation path when `show_in_fullscreen() == true`. When the setting is `false` (default) and a recording is in flight at the moment of fullscreen-entry, abort the recording silently — no transcript event, no paste, dictation core returns to idle.

**Files.** `apps/tauri/src-tauri/src/lib.rs` (the existing `fullscreen::install(...)` callback), possibly `core/src/dictation/...` (if an `abort` path needs to be added distinct from `stop`).

**Steps.**

1. Read the dictation core's stop/abort API. If an `abort` (drop audio, no transcript, no error) path already exists, use it. If only `stop` (flush + transcribe) exists, add a new `abort()` function in the core that:
   - Halts the recording stream.
   - Discards the captured audio buffer.
   - Resets phase to `IDLE` without emitting `dictation_deliver_transcript`.
   - Emits whatever phase-tick the existing `IDLE` transition does (so the pill flips back to idle visually if visible).
2. Modify the fullscreen callback in `lib.rs:581-585`:
   ```rust
   fullscreen::install(move |is_fullscreen| {
       let bypass = behavior::show_in_fullscreen();
       if bypass {
           // Setting=on — let the detector observe but don't deactivate.
           return;
       }
       hotkey::set_active(&app_for_fullscreen, !is_fullscreen);
       if let Some(pill) = app_for_fullscreen.get_webview_window("pill") {
           let _ = if is_fullscreen { pill.hide() } else { pill.show() };
       }
       if is_fullscreen {
           // Mid-recording entering fullscreen with setting=off: abort.
           let snap = openwhisper_core::dictation::dictation_snapshot();
           if snap.phase() == PHASE_RECORDING {
               openwhisper_core::dictation::abort();
           }
       }
   });
   ```
3. Subscribe to `behavior_show_in_fullscreen_changed` in `setup()` so when the user flips the setting *while a fullscreen app is currently focused*, the pill + hotkey state immediately re-reconciles. Easiest: on event, re-evaluate `(is_fullscreen_now, show_in_fullscreen)` and apply the same logic (no-deactivate vs deactivate).
4. `cargo check` clean.
5. Manual smoke (release build for setting=on path; dev build covers setting=off + abort path):
   - Setting=off + idle → enter fullscreen → pill hides, hotkey suppressed.
   - Setting=off + recording → enter fullscreen → pill hides, hotkey suppressed, recording aborts (no paste appears).
   - Setting=on + idle → enter fullscreen → pill stays, hotkey works.
   - Toggle setting from off→on while in fullscreen → pill reappears, hotkey re-arms.

**Outcome ACs (Backlog).**

- Fullscreen callback reads `behavior::show_in_fullscreen()` and short-circuits when `true`.
- Mid-recording fullscreen entry with setting=`false` aborts the recording silently — no transcript, no paste, returns to idle.
- Toggling the setting while fullscreen is currently active immediately reconciles pill + hotkey state without restarting OW.
- `cargo check` clean; manual smoke passes for the four cases above.

---

### Task 3: macOS pill collection-behavior follows the setting

**Goal.** When the setting flips on, set `pill.set_visible_on_all_workspaces(true)` so the pill is allowed to render over fullscreen Spaces. When it flips off, set it back to `false` so the pill stops following the user across Spaces unrelated to fullscreen apps. Boot-time hydration matches the persisted value.

**Files.** `apps/tauri/src-tauri/src/lib.rs` (the `behavior_show_in_fullscreen_changed` listener, plus boot-time hydration after `settings::load_settings`), possibly `apps/tauri/src-tauri/src/behavior.rs` (a small helper `apply_collection_behavior(app, value)` to keep the `lib.rs` callsites tidy).

**Steps.**

1. Helper in `behavior.rs`:
   ```rust
   pub fn apply_collection_behavior(app: &AppHandle, show: bool) {
       if let Some(pill) = app.get_webview_window("pill") {
           let _ = pill.set_visible_on_all_workspaces(show);
       }
   }
   ```
   Wrap in `#[cfg(target_os = "macos")]` if Tauri's API on Windows is no-op or if there's reason to suppress on Windows. Default to applying on both — Tauri's docs say it's a no-op on platforms that don't support it.
2. In `lib.rs::setup()`, after the cache hydrate from Task 1, call `behavior::apply_collection_behavior(app.handle(), loaded.show_in_fullscreen)`.
3. In the `behavior_show_in_fullscreen_changed` listener (added in Task 2), call `apply_collection_behavior` alongside the pill/hotkey reconciliation.
4. Verify on macOS by toggling on, entering a fullscreen app on a different Space, observing the pill remain visible. Toggle off, observe the pill stays on its own Space again.

**Outcome ACs (Backlog).**

- macOS pill window's `visible_on_all_workspaces` mirrors the setting at boot and on every change.
- Toggling on while a fullscreen app is active brings the pill into the fullscreen Space without restart.
- Toggling off while in fullscreen reverts pill to normal Space behavior.

---

### Task 4: GeneralPane "Behavior" section + Switch + useShowInFullscreen hook

**Goal.** Add a "Behavior" section to General pane (between Appearance and Updates, unless TASK-55.6 has already established a different section the row should fold into; executor's call after reading current `general-pane.tsx`). One row: a Switch labeled "Show in fullscreen apps" with the description from the spec. Switch is wired through a new `useShowInFullscreen()` hook that mirrors `useTheme()` / planned `useAutostart()` shape.

**Files.** `apps/tauri/src/lib/use-show-in-fullscreen.ts` (new), `apps/tauri/src/components/general-pane.tsx` (extend).

**Steps.**

1. New `apps/tauri/src/lib/use-show-in-fullscreen.ts`:
   ```ts
   import { useEffect, useState } from "react";
   import { invoke } from "@tauri-apps/api/core";
   import { listen, type UnlistenFn } from "@tauri-apps/api/event";

   export function useShowInFullscreen() {
     const [enabled, setEnabledState] = useState(false);

     useEffect(() => {
       invoke<boolean>("behavior_get_show_in_fullscreen")
         .then(setEnabledState)
         .catch(() => setEnabledState(false));
     }, []);

     useEffect(() => {
       let unlisten: UnlistenFn | undefined;
       void listen<boolean>("behavior_show_in_fullscreen_changed", (e) =>
         setEnabledState(e.payload),
       ).then((fn) => (unlisten = fn));
       return () => unlisten?.();
     }, []);

     const setEnabled = (next: boolean) =>
       invoke("behavior_set_show_in_fullscreen", { enabled: next });

     return { enabled, setEnabled };
   }
   ```
2. Read current `general-pane.tsx`. If it has a "Behavior" or "Pill" section already (TASK-55.6 might have landed first), add the row there. Otherwise add a new `<Separator />` + `<SectionHeader>Behavior</SectionHeader>` block above the Updates section.
3. Add the row using existing shadcn primitives — same shape as the Launch at login row:
   ```tsx
   <Field orientation="horizontal">
     <FieldContent>
       <FieldLabel htmlFor="show-in-fullscreen">Show in fullscreen apps</FieldLabel>
       <FieldDescription>
         Keeps the pill visible and the hotkey active even when another
         app is in fullscreen. Off by default — most users want OpenWhisper
         to step aside for games and video.
       </FieldDescription>
     </FieldContent>
     <Switch
       id="show-in-fullscreen"
       checked={enabled}
       onCheckedChange={setEnabled}
     />
   </Field>
   ```
4. `pnpm tsc --noEmit` clean from `apps/tauri/`.

**Outcome ACs (Backlog).**

- New `use-show-in-fullscreen.ts` hook uses invoke + listen, matching the project's Settings hook pattern.
- General pane has a Switch row in a Behavior section (or merged into an existing section) with the spec's description copy.
- Toggling the Switch persists, updates the cache, and is reflected back via the listen subscription (next render).
- `pnpm tsc --noEmit` clean.

---

### Task 5: Playwright spec + tauri shim stubs

**Goal.** Cover the React side of the wiring. Mock `behavior_get_show_in_fullscreen` / `behavior_set_show_in_fullscreen` at the shim boundary; assert initial state, write-through, and external-event update via `behavior_show_in_fullscreen_changed`.

**Files.** `apps/tauri/tests/settings-window.spec.ts` (extend), `apps/tauri/tests/fixtures/tauri-shim.ts` (add stubs + helper).

**Steps.**

1. In the tauri shim, add to the invoke handler:
   - `behavior_get_show_in_fullscreen` → returns `window.__owShowInFullscreen ?? false`.
   - `behavior_set_show_in_fullscreen` → writes payload to `window.__owShowInFullscreenLastSet` and (optionally) to `__owShowInFullscreen` so subsequent reads are consistent.
   - Helper `emitShowInFullscreenChanged(page, value)` that dispatches the event.
2. Add to `test.describe("settings view", ...)`:
   - **"Show in fullscreen Switch reflects behavior_get on mount"** — set the shim default to `true`, open Settings, assert Switch is checked.
   - **"Toggling the Switch invokes behavior_set with the new value"** — start unchecked, click, assert `__owShowInFullscreenLastSet === true`.
   - **"behavior_show_in_fullscreen_changed event updates the Switch"** — open Settings, emit the event with `true`, assert the Switch becomes checked.
3. Verify the existing settings tests + Theme + section structure tests + (planned) Launch at login tests still pass.
4. `pnpm test:ui` green.

**Outcome ACs (Backlog).**

- Tauri shim exposes stubs for the two behavior commands plus an `emitShowInFullscreenChanged` helper.
- Three new tests assert: initial state from `behavior_get_show_in_fullscreen`, write-through via `behavior_set_show_in_fullscreen`, external event update via `behavior_show_in_fullscreen_changed`.
- Existing Settings + General-pane tests still pass.
- `pnpm test:ui` green locally and on CI.

---

## Reviewer loop

After all 5 plan tasks have matching Backlog subtasks (`TASK-58.1` through `TASK-58.5`), dispatch the plan-document-reviewer subagent with the standard plan-review criteria PLUS the verbatim Backlog-enforcement fragment from `.claude/skills/writing-backlog-plans/references/plan-reviewer-addendum.md` AND a Tauri-specific check that the abort path for mid-recording fullscreen entry doesn't accidentally fire `dictation_deliver_transcript` (which would land surprise-paste into the fullscreen app — the exact UX trap this spec rejects).

## Execution handoff

Sequential: 1 → 2 → 3 → 4 → 5. Task 4 could parallelize with Task 3 (different concerns), but executor convenience favors strict sequence.

- Task 1 lands the persistence + cache + commands.
- Task 2 wires the detector callback to the cache + adds the abort path.
- Task 3 layers macOS pill collection-behavior on top.
- Task 4 brings the React side online.
- Task 5 covers the React side with tests.

Status updates flow through `backlog task edit` per the cheatsheet. Each subtask appends commit refs in implementation notes (`--append-notes`), checks ACs as they land (`--check-ac`), and ends with a `--final-summary` + `-s Done`.

## TDD shape note

Task 1 is straight schema + glue; cargo check + a small persistence smoke covers it. Tasks 2 and 3 are integration-shaped (Tauri lifecycle, OS detector, AppKit collection-behavior); manual smoke is the pragmatic verification for the cross-OS path, with Playwright covering the React surface in Task 5. Task 4 leans TDD with Task 5 — write the React hook with the shim stubbed, then the tests, iterating until green.

The mid-recording abort path (Task 2 step 1-2) deserves a unit-style test in the Rust core if `abort()` is added there — `cargo test` should cover "abort drops audio + returns to idle without emitting transcript" so we catch any regression that leaks a paste into a fullscreen app down the line.
