---
id: doc-42
title: Home-pane stats — implementation plan
type: specification
created_date: '2026-05-06 06:11'
---

**Backlog parent:** TASK-88
**Spec:** backlog/docs/specs/doc-41 - Home-pane-stats-—-design.md
**Depends on:** TASK-87 (persistence foundation) — all of it must be Done before TASK-88.1 starts.

Seven tasks. Sequencing is partly parallel:

- **Path A (Rust):** T1 (write path) → T2 (read path + reset). Sequential.
- **Path B (settings):** T3 (WPM field) — independent.
- **Path C (React):** T4 (Stats settings pane) depends on T3. T5 (Stats strip + hook) depends on T2 + T3. T6 (in-line link) depends on T4 + T5. T7 (Playwright) depends on T5 + T6.

The fastest path runs T1 + T3 in parallel after TASK-87 lands.

---

### Task 1: Stats writer — record_dictation in core, wired into dictation_deliver_transcript

Adds the write path. Every successful dictation lands one row in `dictations`.

> **Signature contract with T2:** `record_dictation` ships in T1 with NO Tauri-event awareness — just an INSERT and a `tracing::warn!` on failure. T2 will modify this signature to take an `on_insert: impl Fn() + Send + Sync` callback that the shell registers at boot to emit `stats_changed`. The T1 executor should NOT pre-add the callback parameter; T2 owns that change. Keeping T1's commit narrow (write-only, no event surface) makes both reviews tractable and avoids a churning re-edit.

**Files:**

- `core/src/stats/mod.rs` (new) — `pub fn record_dictation(store: &Store, started_at_ms: i64, duration_ms: i64, text: &str)` per spec body. Empty-text early-return. Failures `tracing::warn!` and return.
- `core/src/lib.rs` — add `pub mod stats;` next to `pub mod store;`.
- `core/src/dictation.rs` — modify `dictation_deliver_transcript` (line 321-ish): after `inj.inject(text)`, capture `started_at_ms` from `record_start.elapsed()` math (Instant-to-epoch is awkward — store the epoch start at record-start time instead, see decision below), compute `duration_ms`, call `record_dictation(...)`. The `Store` reaches the function via a new module-level `OnceLock<Arc<Store>>` set by the shell at boot (mirrors the existing `INJECTOR` pattern at `dictation.rs:351`).
- `apps/tauri/src-tauri/src/lib.rs` — in the setup hook (already touched by TASK-87.3 to call `Store::open_or_init`), call a new `core::stats::set_store(arc_store.clone())` to register the global. The shell keeps a `State<Arc<Store>>` for Tauri commands; the dictation core gets its own `Arc<Store>` clone.

**Decision lock — `started_at_ms` source.** The current dictation state stores `record_start: Option<Instant>`, which can't directly produce a unix epoch. Two options:
- **(A)** Capture `SystemTime::now().duration_since(UNIX_EPOCH)` at `dictation_mark_capture_started` and store it alongside `record_start`. Add `record_start_epoch_ms: Option<i64>` to the dictation state.
- **(B)** Use `SystemTime::now()` at `dictation_deliver_transcript` and subtract `record_start.elapsed()`. Slightly wrong if elapsed math drifts but always close.
- **Pick (A).** Cheap, exact, and the field has obvious value for future history features. One extra `i64` in the state.

**Outcome ACs:**

- `core::stats::record_dictation` exists with the signature above; empty-text input returns without inserting.
- `core::stats::set_store(Arc<Store>)` exists; idempotent (first call wins, mirrors `set_injector`).
- `dictation_deliver_transcript` captures `started_at_ms` (via the new state field) + `duration_ms` and calls `record_dictation` after `inj.inject`.
- Cancel and empty-transcript paths reach NO insert — verified by a unit test that walks `dictation_mark_capture_stopped(0)` (no audio) followed by `dictation_deliver_transcript("")` and asserts zero rows.
- DB-write failure logs at `warn!` and does not transition phase to `PHASE_ERROR` (verified by injecting a closed-store handle in a test).

**Verification:**

- `cargo test -p openwhisper-core stats::` covers: happy-path insert, empty-text no-op, failure-no-panic.
- Manual smoke during `pnpm tauri dev`: record one dictation, then in DevTools `await window.__TAURI__.core.invoke("stats_get_summary")` (after T2 lands) returns words > 0.

---

### Task 2: Read path — `stats_get_summary` + `stats_reset` Tauri commands + `stats_changed` event

Adds the read side and the reset side. Emits `stats_changed` after each insert (T1 site) and after reset.

**Files:**

- `core/src/stats/mod.rs` — add `pub fn get_summary(store: &Store, now_ms: i64) -> Result<StatsSummary, StoreError>` per spec. `now_ms` passed in (not `SystemTime::now()` inline) so tests can pin time. Day boundary computed via `chrono::Local`.
- `core/src/stats/mod.rs` — add `pub fn reset(store: &Store) -> Result<(), StoreError>` (DELETE FROM dictations).
- `core/Cargo.toml` — add `chrono = { version = "0.4", default-features = false, features = ["clock"] }` if not already present (`clock` enables `Local`). Verify it's not already a transitive dep.
- `apps/tauri/src-tauri/src/lib.rs` — add `#[tauri::command] fn stats_get_summary(...)` and `#[tauri::command] fn stats_reset(...)`. Both pull `State<Arc<Store>>`. Reset emits `app_handle.emit("stats_changed", ())` after the DELETE.
- `core/src/stats/mod.rs` — `record_dictation` (from T1) gains a callback: accept a `Fn() + Send + Sync` "on insert" hook, called after a successful insert. The shell registers a hook at boot that calls `app_handle.emit("stats_changed", ())`. This keeps core unaware of Tauri events while still letting the shell react.
- Register both commands in the `invoke_handler!` macro at `apps/tauri/src-tauri/src/lib.rs:1181`.

**Outcome ACs:**

- `stats_get_summary` returns `StatsSummary { words_today, words_week, words_all_time, seconds_total }` with all four aggregates correct against fixture data covering: zero rows, rows from yesterday, rows from earlier this week, rows from a year ago.
- `stats_reset` empties the table; subsequent `stats_get_summary` returns the empty summary.
- `stats_changed` event fires within 50 ms of a successful `record_dictation` insert AND within 50 ms of a successful `stats_reset`.
- Day-boundary math handled by `chrono::Local` — no manual UTC offset arithmetic in the codebase.

**Verification:**

- `cargo test -p openwhisper-core stats::` covers all summary-correctness cases listed above.
- Manual smoke: record a dictation, observe `stats_changed` fire in DevTools console (`window.__TAURI__.event.listen("stats_changed", console.log)`).

---

### Task 3: WPM setting — add `user_wpm: u32` to settings store with clamp validation

Independent of T1 / T2; can run in parallel with T1.

**Files:**

- `apps/tauri/src-tauri/src/settings/mod.rs` — add `user_wpm: u32` field to the settings struct with `#[serde(default = "default_wpm")] fn default_wpm() -> u32 { 40 }`.
- Same file — extend the existing settings setter (whatever shape `settings_set_*` takes) so writes pass through a `clamp(10, 300)` for `user_wpm`. Out-of-range input is silently clamped; no error surface. Helper text (which lives in React) communicates the bounds.
- `apps/tauri/src/lib/use-settings*.ts` (existing hooks dir, exact path TBD on read) — expose `userWpm: number` and `setUserWpm: (n: number) => Promise<void>` in whatever shape the existing settings hook uses for other fields.
- Migration: any existing settings file without `user_wpm` defaults to `40` via serde default. No explicit migration code needed.

**Outcome ACs:**

- `user_wpm` field exists in the settings JSON store; absent in older files defaults to `40` on read.
- Settings setter clamps writes to `[10, 300]`; a `set_user_wpm(5)` followed by `get_user_wpm` returns `10`; `set_user_wpm(500)` returns `300`.
- React hook exposes the value and setter; no Tauri-level error surfaces for out-of-range input.

**Verification:**

- `cargo test -p tauri-shell settings::` (or whatever the shell crate name is) — happy-path read/write + clamp behavior.
- Manual smoke: edit settings file by hand to inject `user_wpm: 9999`, relaunch app, confirm the value reads as `300`.

---

### Task 4: Stats settings pane — register, render WPM input + Reset Stats button

Depends on T3 (WPM hook) and T2 (`stats_reset` cmd).

**Files:**

- `apps/tauri/src/lib/settings-panes.ts` — insert `{ id: "stats", label: "Stats" }` between `models` and `shortcuts`. Order matters; sidebar reflects this order.
- `apps/tauri/src/components/sidebar-nav.tsx` — extend the icon map to associate `stats` with lucide `BarChart3`.
- `apps/tauri/src/Settings.tsx` (or wherever the active-pane switch lives) — route the `stats` pane to a new `<StatsPane>`.
- `apps/tauri/src/components/stats-pane.tsx` (new) — two sections:
  - "Typing speed" — shadcn `Field` + `FieldLabel` + numeric `Input` (min 10, max 300, step 1) + `FieldDescription` with the **exact** spec helper text: *"Used for the Time Saved estimate on Home. The default 40 wpm is an average adult baseline. If unsure, take a free online typing test and enter the result."* (Do not paraphrase — this copy is locked in spec doc-41 §"Stats settings pane.")
  - "Danger zone" — shadcn `Card` with `className="border-destructive/40 bg-destructive/5"`, `CardTitle` "Danger zone" (`text-destructive`), `CardDescription` "These actions are irreversible.", `CardContent` containing `Button variant="destructive"` "Reset all stats…" + shadcn `AlertDialog` for confirm. On confirm, `await invoke("stats_reset")` then call `refresh()` on the stats hook.
- Use `FieldGroup` to wrap the Typing-speed section per the shadcn forms rule. The Danger zone is a sibling Card, not a Field — destructive levers do not belong in form composition primitives.

**Outcome ACs:**

- New "Stats" pane appears in the Settings sidebar between "Models" and "Shortcuts"; selecting it renders the two sections.
- Editing the WPM input persists through reload.
- Clicking the destructive button shows the AlertDialog with the spec's confirm copy; confirming clears all rows; canceling does nothing.
- After a successful reset, the Home pane's stats strip re-renders to empty state within one event-loop tick (verified by the Playwright spec in T7).

**Verification:**

- `pnpm tauri dev` — exercise both sections manually.
- `pnpm test:ui` still green (no new spec yet; just no regression).

---

### Task 5: `<StatsStrip>` component on Home pane + `useStatsSummary` hook

Depends on T2 (read cmd + event) and T3 (WPM value for time-saved calc).

**Files:**

- `apps/tauri/src/lib/use-stats-summary.ts` (new) — hook that invokes `stats_get_summary` on mount and re-fetches on every `stats_changed` event. Returns `{ summary: StatsSummary | null, refresh }`.
- `apps/tauri/src/components/stats-strip.tsx` (new) — 4 cards in a `grid grid-cols-4 gap-3`. Each card: small uppercase label, large value, subcaption. Use shadcn `Card` composition (`CardHeader` + `CardContent`).
- Helper functions colocated: `formatTimeSaved(secs)` per spec body, `dotSeparator()` not needed here (this is the strip, not the footer).
- `apps/tauri/src/components/home-pane.tsx` — render `<StatsStrip>` above the existing hero section (`section.ow-home__hero`). Strip sits after the banner stack and before the hero.
- Empty state: when `summary` is null OR all word totals are zero, the Time Saved value is `—` and the subcaption simplifies (link suppressed — actual link wiring is T6).

**Outcome ACs:**

- StatsStrip renders 4 cards in the canonical Tailwind grid; each card uses shadcn `Card`/`CardContent` (no styled `<div>` substitutes per the UI-discipline skill).
- After a dictation, the values update without a manual refresh (verified by Playwright in T7).
- Empty state shows `0 / 0 / 0 / —` with subcaptions matching the mockup.
- WPM changes in Settings update the Time Saved card immediately (verified manually here, by Playwright in T7).

**Verification:**

- `pnpm tauri dev` — record dictations, watch the strip update; change WPM, watch Time Saved recompute.
- `pnpm build` clean; `pnpm test:ui` still green.

---

### Task 6: In-line wpm link in the Time Saved subcaption

Depends on T4 (Stats pane exists as a route target) and T5 (the card to attach the link to).

**Files:**

- `apps/tauri/src/components/stats-strip.tsx` — extend the Time Saved card's subcaption rendering: when summary has nonzero rows, render shadcn `Button variant="link"` per the spec's exact JSX, with the `Settings` lucide icon at `data-icon="inline-end"` and `h-auto p-0 text-xs font-normal` className override. When empty, render plain "vs. typing" with no link.
- `apps/tauri/src/components/home-pane.tsx` — accept and forward `onNavigateToStatsSetting` prop down to `<StatsStrip>`.
- `apps/tauri/src/App.tsx` — pass `onNavigateToStatsSetting={() => { setRoute("settings"); setSettingsPane("stats"); }}` to `<HomePane>`.

**Outcome ACs:**

- "vs. typing at <wpm> wpm" renders with the wpm portion as a `Button variant="link"` containing the lucide `Settings` icon at the inline-end position (per shadcn icon rule).
- Clicking the link routes to `route="settings"` AND sets `settingsPane="stats"`.
- When stats are empty (all-time words = 0), the subcaption renders plain text "vs. typing" — no link, no number, no icon.
- Link styling matches the existing button.tsx `link` variant tokens (`text-primary underline-offset-4 hover:underline`); no custom color overrides.

**Verification:**

- Manual smoke during `pnpm tauri dev`: empty state → plain text. After one dictation → link with current WPM. Click → land on Stats pane. Edit WPM → return to Home → link reflects the new value.

---

### Task 7: Playwright coverage for stats — empty / increment / reset / link click

Final task. CLAUDE.md mandates Playwright execution (not inferred) for any change touching the rebind / settings flows; the WPM input qualifies.

**Test-harness reality check:** Playwright runs against the Vite dev server, NOT a real Tauri binary — there is no Rust core in the loop. The existing fixture `apps/tauri/tests/fixtures/tauri-shim.ts` exposes `emitTick(page, {...})` (used in `home.spec.ts:43+`) for driving `dictation_tick` events into the React side. There is no equivalent helper for `stats_get_summary` returns or `stats_changed` events yet — this spec adds them.

**Files:**

- `apps/tauri/tests/fixtures/tauri-shim.ts` (extend) — add a helper `mockStatsSummary(page, summary)` that intercepts the `invoke("stats_get_summary")` call and returns the supplied summary, plus a helper `emitStatsChanged(page)` that fires the `stats_changed` event so the `useStatsSummary` hook re-fetches.
- `apps/tauri/tests/stats.spec.ts` (new) — four test cases:
  1. **Empty state**: `mockStatsSummary(page, { words_today: 0, words_week: 0, words_all_time: 0, seconds_total: 0 })`, navigate to `/`, assert all four cards visible with `0 / 0 / 0 / —` and the Time Saved subcaption is plain `vs. typing` (no link).
  2. **Increment after dictation**: start with the empty mock, navigate to `/`, then update the mock to `{ words_today: 16, words_week: 16, words_all_time: 16, seconds_total: 7.4 }` and call `emitStatsChanged(page)`. Assert the four cards update to nonzero values and the Time Saved subcaption now contains a `Button` element with the link variant.
  3. **Reset stats**: from non-empty state, navigate to Settings → Stats, click Reset, confirm dialog. Assert that `invoke("stats_reset")` was called once and that swapping the mock back to empty + emitting `stats_changed` returns the strip to empty state.
  4. **Link click**: from Home (non-empty mock), click the in-line wpm link in the Time Saved card; assert the route flips to `settings` and the active pane is `stats` (use the same route-detection signal `home.spec.ts` uses or its sibling specs).

**Outcome ACs:**

- Spec exists with all four cases above; `pnpm test:ui` discovers and runs it.
- All four cases pass — actually executed, not inferred from reading the file (per CLAUDE.md).
- A regression that breaks the stats_changed event subscription (T2/T5) is caught by case 2 (no auto-update).
- A regression that decouples the in-line link from the WPM setter is caught by case 4 (link click goes to wrong pane).

**Verification:**

- `pnpm test:ui` runs and reports all stats spec cases as passing on macOS at minimum (Windows in CI).

---

## Cross-task notes

- **Parallelism:** T1 + T3 can run in parallel. T2 needs T1. T4 needs T2 + T3. T5 needs T2 + T3. T6 needs T4 + T5. T7 needs T5 + T6.
- **All of TASK-87 must be Done before T1 starts.** That is the load-bearing dep. No code in TASK-88 reaches in to the schema.
- **No deferred design decisions.** Storage shape, formula, formatting, link icon, pane id, WPM bounds — all locked.
- **Out-of-scope reminders:** No history list, no per-app breakdown, no charts, no built-in typing test, no transcript writes from this feature, no SQLite writes for the WPM setting (it lives in JSON).
- **Executor reminder:** before editing any file under `apps/tauri/src/components/` or adding controls, load the `openwhisper-ui-discipline` skill (which loads `shadcn`). Confirm shadcn-canonical primitives are used: `Field`/`FieldGroup` for the WPM form, `AlertDialog` for the destructive confirm, `Card` for stats cards, `Button variant="link"` for the in-line link.
