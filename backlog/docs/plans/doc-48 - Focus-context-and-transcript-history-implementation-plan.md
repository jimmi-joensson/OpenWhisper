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
- General settings has "Reset stats", which deletes every row in `dictations` (counters only; history is separate files).

## Design decisions

1. **Focus context lives in core** (`core/src/focus/`), behind a Cargo feature `focus` that `tauri` pulls in and the CLI enables, so the SwiftUI staticlib flavor gains no new dependencies. One call, `focus::current()`, returns `FocusContext { app_id, app_name, window_title, pid, is_self }`. Every field is best effort: a piece that cannot be read is empty, and the call never fails.
2. **macOS read.** App id and name from `NSWorkspace.frontmostApplication` (no permission). Window title through the Accessibility API (`AXFocusedWindow` then `AXTitle`), which works when the app has the Accessibility grant it already needs for the hotkey; empty otherwise. A messaging timeout on the element keeps a hung app from stalling the read.
3. **Windows read.** `GetForegroundWindow`, then the owning process's exe path. `app_id` is the exe file name in lower case (`slack.exe`), `app_name` is the file's `FileDescription` (falls back to the exe name without extension), title from `GetWindowTextW`.
4. **Own windows.** `is_self` is true when the frontmost process is this process. History stores no app for such a dictation (the text only went to the clipboard).
5. **When it is read.** In `dictation_deliver_transcript`, right after the injector is called and only when history is on. The injector pastes from its own thread and a paste does not change focus, so the read cannot delay the paste and still names the app that receives the text.
6. **The store is a folder of Markdown files, one per day** (decided with the user on 2026-11-10, replacing a first version that kept the text in SQLite). Reason: the text should be readable and searchable by other programs, a coding agent above all, without OpenWhisper. Layout `history/<YYYY>/<YYYY-MM-DD>.md`. Each file has YAML frontmatter that sums up the day (`date`, `entries`, `words`, `apps`), then one block per dictation: a `## HH:MM:SS · App` heading, a comment line with the machine fields (`id`, `app`, `words`, `seconds`), the text, and the text before Refine as a `> Before Refine:` quote when Refine changed it. The window title is not stored. The format is documented at the top of `core/src/history/mod.rs`.
7. **The files are the only copy of the text.** The SQLite store keeps counters for the stats and no text (no schema change: the migration and `secure_delete` from the first version are gone). Daily, not weekly or per message: an agent can open today's file by name, retention is "remove old days", and thousands of tiny files are avoided.
8. **Hand edits and shared folders are safe.** Existing content is never re-rendered: adding an entry appends one block, removing entries cuts their blocks out. Text before the first entry (not written by OpenWhisper) and extra frontmatter keys are kept, and a file that still has such text is never deleted. Only files named `<YYYY>/<YYYY-MM-DD>.md` are read or touched. Edited text is what the app shows.
9. **Retention.** `history.retention_days` in settings.json: 7, 30, 90 or 0 for forever; default 30. Whole days: with 30, a dictation is kept through the 30th day after the one it was made on. Pruning runs after every delivered dictation, at app start and when the setting is saved, also while history is switched off.
10. **One switch, no separate pause.** Off stops saving and keeps what is stored until it is cleared or retention removes it.
11. **Folder.** Default `history/` in the app's config folder, next to `dictionary.json`. `history.folder` in settings.json moves it (Settings has a folder picker, the CLI has `--folder`). Files already written are not moved. The settings say that a synced folder syncs the dictations.
12. **No index.** Search is a scan in Rust (lower-cased substring over text, original text and app name, so Danish letters fold correctly). Three years of dictation is a few megabytes; parsed files are cached by modification time and size. An index can come later if it is ever slow.
12a. **Ids** are `YYYYMMDDTHHMMSS` of the recording's start in local time (`-2`, `-3` if two share a second), so an id names its day file.
12b. **What is given up.** Removed text is not overwritten on disk (plain files; backups and sync may keep copies). Two programs writing the same day file at the same moment can lose one write; the app serializes its own writers and replaces files atomically.
12c. **Agents.** The `AGENTS.md` the app writes next to `dictionary.json` gets a section describing the folder, the format and how to search it. Reset stats no longer touches history.
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

Files: `core/src/history/{mod,tests}.rs` (new: the Markdown store with `append`, `list`, `get`, `delete`, `clear`, `prune`), `core/src/settings/history.rs` (new), `core/src/settings/mod.rs` (block, load, save), `core/assets/dictionary-agents.md`, `core/src/lib.rs`.

Verification: `cargo test -p openwhisper-core --no-default-features --features tauri --lib`.

## TASK-95 Task 2: Pipeline write and retention (TASK-95.2)

Files: `core/src/dictation.rs` (`dictation_deliver_transcript` appends to the day file and prunes, before the stats write).

Verification: the store's unit tests; dictating through the Dev app.

## TASK-95 Task 3: `openwhisper history` (TASK-95.3)

Files: `cli/src/commands/history.rs` (new), `cli/src/commands/mod.rs`, `cli/src/main.rs`.

`openwhisper history list [--search Q] [--limit N] | search Q | show <id> | delete <id> | clear --yes | config [--enabled BOOL] [--retention-days N] [--folder PATH | --default-folder]`, all with `--json`.

## TASK-95 Task 4: Tauri commands (TASK-95.4)

Files: `apps/tauri/src-tauri/src/history.rs` (new), `apps/tauri/src-tauri/src/lib.rs`.

`history_get`, `history_set`, `history_list`, `history_delete`, `history_clear`, `history_reveal`; default folder registered, settings hydrated and retention applied at boot.

## TASK-95 Task 5: Home list, Settings section, quick switches (TASK-95.5, TASK-100)

Files: `apps/tauri/src/lib/{history,use-history,query-client,use-refine-enabled}.ts`, `apps/tauri/src/components/{home-history,history-pane,history-row,history-settings,home-switches}.tsx` (new); `home-pane.tsx`, `general-pane.tsx`, `transcript-row.tsx`, `crash-row.tsx`, `sidebar-nav.tsx`, `App.tsx`, `main.tsx`, `App.css`.

Controls (shadcn, per `openwhisper-ui-discipline`): `Switch` for on/off, `Select` for retention (four fixed choices), `Input` for search, `AlertDialog` to confirm clear all.

## TASK-95 Task 6: Tests and verification (TASK-95.6)

Files: `apps/tauri/tests/history.spec.ts` (new), `apps/tauri/tests/fixtures/tauri-shim.ts`.

Verification: core, CLI and shell cargo tests; Windows target check; `pnpm exec playwright test`; then dictating through the Dev app with history on and off.
