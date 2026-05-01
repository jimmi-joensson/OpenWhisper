# Home pane + sidebar nav — v1 spec

**Date:** 2026-05-01
**Backlog parent:** TASK-65
**Design source:** Claude Design handoff bundle (`/tmp/openwhisper-handoff/openwhisper/`), artboard `main-window` ("Main window — wired to global state"). User pivoted from the mock's debug-style composition to a clean home screen; this spec captures the deviation.

---

## Why this exists

The current `MainWindowShell` reads as a developer dashboard — four KV cards, a 32-bar meter, a transcript box, a record button. That made sense during the Tauri port spike (TASK-31..38) when the goal was "prove every wire works end-to-end". Post-v0.4.0 the app needs a home screen that reads as a product, not a probe. The dashboard surface still has value for users reporting bugs, so it's relocated rather than removed.

## Outcome

When a user opens OpenWhisper for the first time after this lands, they see:

1. A left rail with three items: **Home**, **Settings**, **Diagnostics**.
2. On Home: a hero with the bundled app icon, the headline "Ready when you are", and a sub-line that reads (mac default) "Press **Right ⌘** anywhere to speak. Press it again to stop." The chord in bold pulls live from the user's current toggle binding — rebinding in Settings updates the hint without a reload.
3. After the user dictates once and the phase returns to idle with a non-empty transcript, a single row appears below the hero in history-row style. The row shows the transcript text, a relative timestamp ("just now" / "2m ago"), and a hover-revealed copy-to-clipboard button. Each subsequent dictation **replaces** the row — there is no list.
4. On Diagnostics: the existing `MainWindowShell` dashboard, unchanged. Always visible (not DEV-gated), so a user filing a bug report can copy the FFI version, phase, perms state, and last error without a special build.
5. Settings is unchanged internally; only the entry point moves from a titlebar gear icon to the sidebar.

## Why each open question landed where it did

- **No transcript history.** User explicit: v1 shows one row, the most recent. Persistence, list cap, SQLite, scrolling — all deferred. Adding any of that now would invent behavior the user hasn't asked for and would force a database decision before we know the read patterns.
- **HealthBanner at the top of Home, not inside a shell.** Errors are global (mic denied, hotkey can't register, recognizer load failed). They affect the whole app, not just one pane. Putting them above the hero means a user landing on Home sees the failure on first paint and can act on it; tucking them into Diagnostics would hide failures behind a navigation step.
- **Diagnostics in sidebar, not DEV-gated.** Users reporting bugs need a surface to copy from on shipped builds. A DEV-only panel is useless to them. The cost is one always-visible sidebar item that most users will never click; the benefit is "send me a screenshot of Diagnostics" stays a viable triage path.
- **Hover-only copy, no row click.** Click-to-copy is invisible affordance and risks accidental fires; click-to-reinsert is destructive (re-emits keystrokes into wherever focus is) and complicates the row's mental model. Hover button = explicit + safe + matches the design tokens.
- **Live hotkey hint, not hardcoded.** "Right ⌘" lies after a user rebinds in Settings → Shortcuts. The binding is already exposed via `settings_get_hotkeys`; subscribing to `hotkey_captured` keeps the hint in sync without a reload. Hardcoding "Right ⌘" in the hero would be the third place the same string lives (alongside the Shortcuts pane and the tray menu accelerator) and would drift.
- **Bundle PNG icon, not inline SVG.** The icon is already the source of truth for the dock, menubar, installer, and tray. Reusing it from `src-tauri/icons/` means the hero matches whatever icon shipped that build. An inline SVG would be a fourth surface to keep aligned. ~64×64 in the hero — the bundle ships down to 32×32, so 64×64 picks the next-up size and lets browser rendering downsample.
- **Outer sidebar is its own grid, Settings keeps its inner sub-sidebar.** The Settings sub-sidebar (General/Audio/Models/Shortcuts) is a different navigational level — pane within Settings, not top-level. Collapsing both into one sidebar would either bury Audio/Models/Shortcuts as second-class items or force a tree control. Keeping them nested is honest about the hierarchy and means Settings.tsx ports forward without re-architecture.

## Out of scope (v1)

- Transcript history list, scroll, search, persistence (SQLite / JSON / anywhere).
- Stats, counters, "you've dictated N words this week".
- Re-insert from row, click-to-copy on the row body, drag-to-reorder.
- Multi-row history rendering, even capped at 2.
- Sidebar collapse / resize / icon-only mode.
- Any new Rust commands. The hotkey hint, the latest-transcript state, and the sidebar nav are all derivations of data the React shell already receives (`settings_get_hotkeys`, `hotkey_captured`, `dictation_tick`).
- ow_navigate accepting "diagnostics" as a payload from Rust. The tray menu's existing `Preferences…` / `Show OpenWhisper` items keep their current targets ("settings" / "main"); Diagnostics is sidebar-only in v1.

## Constraints carried forward

- **Tauri shell stays UI-only.** The dictation phase machine, hotkey gating, transcript filtering — all stay in Rust core. Per the openwhisper-orchestration-in-rust skill, the React layer derives presentation state from existing tick events; it does not add new state machines or phase semantics.
- **Existing Playwright suite must stay green.** `apps/tauri/tests/main-window.spec.ts` has 8 tests asserting the current dashboard. Refactor splits them: home-specific assertions move to `home.spec.ts`, debug-card assertions move to `diagnostics.spec.ts`, shell-level assertions (scroll, drag, sidebar) stay in `main-window.spec.ts`.
- **Two-attempt iteration budget.** Per the openwhisper-iteration-budget skill, each subtask gets at most one initial implementation + one feedback-driven fix before stopping for research. Plan tasks are sized so this budget is realistic.
