---
id: doc-48
title: Focus context and transcript history - implementation plan
type: other
created_date: '2026-11-10T14:39:00Z'
---

**Backlog parents:** TASK-94 (Focus context), TASK-95 (Transcript history), TASK-100 (Home quick switches)
**Spec:** none as a separate file. Decisions were taken with the user on 2026-11-10 and are recorded in the task descriptions and under "Design decisions" below.
**Roadmap this belongs to:** TASK-94 to TASK-99 (focus context, history, modes, ask, vision, agent). Only 94, 95 and 100 are planned here.

## What exists today (verified in code, 2026-11-10)

- `core/src/store/migrations.rs`: table `dictations` has `transcript` and `app_bundle_id` columns. `core/src/stats/mod.rs` `record_dictation` always writes NULL into both.
- `core/src/dictation.rs` `dictation_deliver_transcript`: sets the snapshot, calls the injector, then writes the stats row. The unrefined text is on the snapshot (`RefineInfo.raw_transcript`) when Refine ran.
- Home (`apps/tauri/src/components/home-pane.tsx`) shows one row, the latest transcript, from memory (`use-last-transcription.ts`).
- No code reads the frontmost app's identity. `apps/tauri/src-tauri/src/injection` only asks whether our own window is focused and, on Windows, whether the foreground window is Chromium.
- General settings has "Reset stats", which deletes every row in `dictations`.

## Design decisions

1. **Focus context lives in core** (`core/src/focus/`), behind a Cargo feature `focus` that `tauri` pulls in and the CLI enables, so the SwiftUI staticlib flavor gains no new dependencies. One call, `focus::current()`, returns `FocusContext { app_id, app_name, window_title, pid, is_self }`. Every field is best effort: a piece that cannot be read is empty, and the call never fails.
2. **macOS read.** App id and name from `NSWorkspace.frontmostApplication` (no permission). Window title through the Accessibility API (`AXFocusedWindow` then `AXTitle`), which works when the app has the Accessibility grant it already needs for the hotkey; empty otherwise. A messaging timeout on the element keeps a hung app from stalling the read.
3. **Windows read.** `GetForegroundWindow`, then the owning process's exe path. `app_id` is the exe file name in lower case (`slack.exe`), `app_name` is the file's `FileDescription` (falls back to the exe name without extension), title from `GetWindowTextW`.
4. **Own windows.** `is_self` is true when the frontmost process is this process. History stores no app for such a dictation (the text only went to the clipboard).
5. **When it is read.** In `dictation_deliver_transcript`, right after the injector is called and only when history is on. The injector pastes from its own thread and a paste does not change focus, so the read cannot delay the paste and still names the app that receives the text.
6. **History reuses the `dictations` row.** Migration 5 adds `raw_transcript` and `app_name`. With history off the row is written exactly as before. With history on: `transcript` = final text, `raw_transcript` = the unrefined text only when Refine changed it, `app_bundle_id` + `app_name` from the focus context. The window title is not stored.
7. **Removing text keeps the stats row.** Delete, clear and retention set the four history columns to NULL. `PRAGMA secure_delete = ON` so removed text is overwritten in the file instead of lingering in free pages.
8. **Retention.** `history.retention_days` in settings.json: 7, 30, 90 or 0 for forever; default 30. Pruning runs after every write, at app start and when the setting is saved, also while history is switched off.
9. **One switch, no separate pause.** Off stops saving and keeps what is stored until it is cleared or retention removes it. That is what a pause would do, so there is only one control.
10. **Search** is done in Rust (lower-cased substring over final text, original text and app name) because SQLite `LIKE` only folds ASCII case and Danish text needs more. Fine at this size: text for tens of thousands of dictations is a few megabytes.
11. **Change signal.** History changes reuse the existing `stats_changed` event (every insert already fires it), so the webview needs no second listener contract.
12. **Reset stats also removes history**, because it deletes the rows. The confirmation says so.
13. **Home** (revised with the user on 2026-11-10, after trying the first version). A quick-switch row with Refine and History, each with a shortcut to its settings (TASK-100). Below it the newest 5 stored dictations with copy, delete and show original, and a link to the History screen. With history on, the dictation that just finished is the first entry of that list and carries the dictionary fixes and the Refine marker; with history off the latest transcript keeps its own row as before.
14. **History screen.** A sidebar item of its own: every stored dictation grouped by day, search, pages of 25, clear all. Clear all lives here and not in Settings.
14a. **Settings.** General gets a History section: the switch, the retention select, and what is stored and where the file is.
14b. **Data hooks on TanStack Query.** The history hooks use one query cache instead of per-hook state, so the switch on Home and the one in Settings share one value and the lists re-read when Rust signals a change. The other hooks move later (TASK-101).
14c. **One hover-reveal pattern.** `.ow-hover-host` / `.ow-hover-reveal` / `.ow-hover-rest` in App.css: `visibility` with no fade, shown for row hover and for keyboard focus (`:focus-visible`). Used by history rows, the latest row and the crash list. Reason: row buttons were seen staying visible in the macOS webview, and `:focus-within` kept them up after a mouse click in Chromium.
15. **Not in this plan.** Audio, export, re-running Refine on old entries, filtering by app, storing the window title, encryption at rest.

## TASK-94 Task 1: Core focus module, type and macOS read (TASK-94.1)

Files: `core/src/focus/{mod,mac}.rs` (new), `core/Cargo.toml` (feature `focus`, macOS deps), `core/src/lib.rs`.

Verification: `cargo test -p openwhisper-core --no-default-features --features tauri focus`, then the CLI command from Task 3 against a few real apps.

## TASK-94 Task 2: Windows read (TASK-94.2)

Files: `core/src/focus/windows.rs` (new), `core/Cargo.toml` (Windows deps).

Verification: `cargo check -p openwhisper-core --no-default-features --features tauri --tests --target x86_64-pc-windows-gnu`. Running it needs the Windows machine.

## TASK-94 Task 3: `openwhisper focus` (TASK-94.3)

Files: `cli/src/commands/focus.rs` (new), `cli/src/commands/mod.rs`, `cli/src/main.rs`, `cli/Cargo.toml`.

`openwhisper focus [--delay-ms N] [--json]`. The delay exists because without it the frontmost app is always the terminal.

## TASK-95 Task 1: Store and settings (TASK-95.1)

Files: `core/src/store/migrations.rs` (migration 5), `core/src/store/mod.rs` (secure_delete, version tests), `core/src/history/mod.rs` (new: `HistoryEntry`, `HistoryPage`, `list`, `get`, `delete`, `clear`, `prune`), `core/src/settings/history.rs` (new), `core/src/settings/mod.rs` (block, load, save), `core/src/lib.rs`, `core/src/prelude.rs`.

Verification: `cargo test -p openwhisper-core --no-default-features --features tauri --lib`.

## TASK-95 Task 2: Pipeline write and retention (TASK-95.2)

Files: `core/src/stats/mod.rs` (`DictationRecord`, `record`), `core/src/dictation.rs` (`dictation_deliver_transcript`).

Verification: unit tests for the row contents with history on and off, and for pruning on write.

## TASK-95 Task 3: `openwhisper history` (TASK-95.3)

Files: `cli/src/commands/history.rs` (new), `cli/src/commands/mod.rs`, `cli/src/main.rs`.

`openwhisper history list [--search Q] [--limit N] | show <id> | delete <id> | clear --yes | config [--enabled BOOL] [--retention-days N]`, all with `--json`.

## TASK-95 Task 4: Tauri commands (TASK-95.4)

Files: `apps/tauri/src-tauri/src/history.rs` (new), `apps/tauri/src-tauri/src/lib.rs`.

`history_get`, `history_set`, `history_list`, `history_delete`, `history_clear`; settings hydrated and retention applied at boot.

## TASK-95 Task 5: Home list, Settings section, quick switches (TASK-95.5, TASK-100)

Files: `apps/tauri/src/lib/{history,use-history,query-client,use-refine-enabled}.ts`, `apps/tauri/src/components/{home-history,history-pane,history-row,history-settings,home-switches}.tsx` (new); `home-pane.tsx`, `general-pane.tsx`, `transcript-row.tsx`, `crash-row.tsx`, `sidebar-nav.tsx`, `App.tsx`, `main.tsx`, `App.css`.

Controls (shadcn, per `openwhisper-ui-discipline`): `Switch` for on/off, `Select` for retention (four fixed choices), `Input` for search, `AlertDialog` to confirm clear all.

## TASK-95 Task 6: Tests and verification (TASK-95.6)

Files: `apps/tauri/tests/history.spec.ts` (new), `apps/tauri/tests/fixtures/tauri-shim.ts`.

Verification: core, CLI and shell cargo tests; Windows target check; `pnpm exec playwright test`; then dictating through the Dev app with history on and off.
