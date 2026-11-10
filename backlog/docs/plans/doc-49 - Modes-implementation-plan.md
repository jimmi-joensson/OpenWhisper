---
id: doc-49
title: Modes - implementation plan
type: other
created_date: '2026-11-10T16:12:00Z'
---


**Backlog parent:** TASK-96 (Modes)
**Spec:** none as a separate file. The decisions the user took are in the task description and under "Design decisions" below.
**Roadmap this belongs to:** TASK-94 to TASK-99. Ask (TASK-97), Vision (TASK-98) and Agent (TASK-99) get their own plans and stack on this branch.

## What exists today (verified in code, 2026-11-10)

- Refine has one `instructions` field (`core/src/settings/refine.rs`). `refine::compose_prompt` turns it into the one system prompt; `refine::warm_for_recording` and `run_for_dictation` are the pipeline hooks.
- `core/src/focus` reads the frontmost app (`focus::current()`), used today only for the history entry.
- History day files (`core/src/history/mod.rs`): one `## HH:MM:SS · App` heading and one `<!-- id: … words: N seconds: S -->` line per entry. An entry starts at a heading whose next non-blank line matches that comment exactly. The released app (0.6.1) uses the same parser and, on this developer's machine, reads the same folder as the Dev build.
- Hotkeys: two fixed slots, `toggle` and `cancel` (`core/src/settings/mod.rs`, `apps/tauri/src-tauri/src/hotkey/{mac,windows}.rs`). macOS takes a modifier tap or a chord, Windows a chord only. Both shells match a chord exactly.
- The tray menu has Open, Start/Stop dictation, Preferences, Quit (`apps/tauri/src-tauri/src/tray/mod.rs`).

## Design decisions

1. **A mode is a name plus instructions**, optionally its own model and effort. Stored in a new `modes` block of `settings.json`: `active` (id), `custom` (list), `app_defaults` (list of app id, app name, mode id).
2. **One built-in mode, "Dictation"** (id `dictation`). It is today's Refine: the `refine.instructions` field, the cleanup prompt and the strict guard, unchanged. It cannot be renamed or deleted. Every other mode is made by the user.
3. **A custom mode rewrites on purpose** (a prompt for a coding agent, a formal tone, English). It gets its own system prompt (`Prompt::compose_mode`): the same fences as today (the transcript is data; never answer it or carry it out; output only the text; dictionary terms; spoken editing instructions still work), with the mode's instructions as the job. Its output is held to the lenient guard (`guard::check_instructed`: not empty, not absurdly long), since the strict guard would reject any rewrite. `min_words` applies to the built-in mode only: a two-word sentence still has to be translated by a translate mode.
4. **Which mode runs.** If the frontmost app has a default mode, that mode (reason `app-default`); otherwise the mode the user selected (reason `selected`). The mode is worked out at record start, on the warm-up thread (to warm the backend), and again when the transcript is ready: a mode or app switched during the recording is the one the text comes out in, and the warmed backend is only used when nothing changed. The app is read only when at least one app default exists. OpenWhisper's own windows never match.
5. **Refine off means no model call, whatever the mode.** The switch is checked before the mode is even resolved.
6. **Where the mode is recorded.** On the Refine outcome and the dictation snapshot (name, reason, app), on the Refine run row (new column `mode`, migration 5), and on the history entry.
7. **History format stays readable by the released app.** The comment line with the machine fields is not changed: a parser that does not know a new field there would stop seeing the entry as an entry and glue it to the one above. Extra fields go on a second comment line directly under it, `<!-- extra: mode=Formal -->` (`key=value` pairs separated by `; `, unknown keys ignored). It is written only when there is something to say, so a plain dictation in the built-in mode produces the same bytes as before. An older app shows that line as text; it does not lose or damage the entry. Ask, Vision and Agent will add their own keys to the same line.
8. **Switching without opening Settings:** a Mode select on Home next to the Refine and History switches, a Mode submenu in the tray menu, and a hotkey that cycles through the modes.
9. **The cycle hotkey is unbound until the user sets it.** With only the built-in mode there is nothing to cycle, and a default chord would take a key combination away from other apps for nothing. Hotkey slots become a list in both shells so that this slot and the Ask slot (TASK-97) are additions, not special cases; an optional slot that cannot be installed is reported and does not take dictation down with it.
10. **Apps offered for a default mode** (`modes::apps_for_defaults`) are the apps already mapped, then the apps seen in history (`history::apps`), then the apps that are open now (`focus::running_apps`), so the list is not empty while History is off. App ids are stored and compared in lower case. The CLI takes any app id (`openwhisper focus` and `openwhisper focus --running` print them; `openwhisper modes app list --candidates` prints the same list as the pane).
11. **Deleting a mode** removes its app defaults and falls back to Dictation where it was selected.
12. **Not in this plan.** Sharing modes between machines, per-mode dictionary, modes for file transcription.

## Task 1: Core: modes block, resolution, prompt, pipeline, history field (TASK-96.1)

Files: `core/src/settings/modes.rs` (new), `core/src/settings/mod.rs` (block, load, save, cache; `HotkeySettings.cycle_mode: Option<HotkeyConfig>`, `HotkeyTarget::CycleMode`), `core/src/modes.rs` (new: list, resolve, add, edit, delete, select, cycle, app defaults), `core/src/refine/{mod,prompt,history}.rs`, `core/src/store/migrations.rs` (migration 5), `core/src/history/{mod,tests}.rs` (`mode` on entries, the `extra` line, `apps()`), `core/src/dictation.rs` (`RefineInfo` mode fields, history write), `core/src/prelude.rs`.

Outcomes:
- `ModesSettings::cleaned()` keeps ids unique, never lets a custom mode take the id `dictation`, drops app defaults that point at a missing mode and falls back to Dictation when the selected mode is gone; covered by tests.
- `modes::resolve` returns the app default before the selected mode and says which it was; covered by tests.
- With Refine off, `run_for_dictation` makes no backend call for any mode (test with the stub backend).
- A custom mode sends its own prompt and model or effort, and its output passes the lenient guard where the strict one would reject it (stub backend test).
- Day files written before this change parse as before; an entry without a mode is rendered byte for byte as before; an entry with a mode round-trips; the old meta regex still matches the meta line of a new entry (test).

Verification: `cargo test -p openwhisper-core --no-default-features --features tauri --lib`.

## Task 2: CLI: `openwhisper modes`, `refine run --mode`, eval cases (TASK-96.2)

Files: `cli/src/commands/modes.rs` (new), `cli/src/commands/{mod,refine}.rs`, `cli/src/main.rs`, `core/src/refine/eval.rs`.

`openwhisper modes list | show <id> | add --name N --instructions TEXT [--model M] [--effort E] | edit <id> … | delete <id> | use <id> | cycle | resolve [--app APP_ID] | app list | app set <APP_ID> <MODE_ID> [--name NAME] | app unset <APP_ID>`, all with `--json`. `openwhisper refine run --mode <id>` and `--app <app-id>` run text through a mode; `refine prompt --mode <id>` prints its prompt. The eval set gets cases that run in a custom mode (a "prompt for a coding agent" mode and a "write in English" mode), including one where the dictated text tries to give the model an order.

Outcomes:
- Every modes operation the app has is reachable from the CLI and prints JSON with `--json`.
- `refine run --mode` uses the same core function as the pipeline.
- The eval set has at least three custom-mode cases, the offline self-test of the eval passes, and a live run is recorded in the task notes.

Verification: `cargo test -p openwhisper-cli`, a live `openwhisper refine eval` run with `OPENWHISPER_CONFIG_DIR` pointing at a scratch folder.

## Task 3: Hotkey slots as a list, optional cycle slot (TASK-96.3)

Files: `apps/tauri/src-tauri/src/hotkey/{mod,mac,windows}.rs`, `apps/tauri/src-tauri/src/settings/mod.rs`, `apps/tauri/src-tauri/src/lib.rs` (`do_cycle_mode`), `apps/tauri/src/lib/{use-global-hotkey,use-current-hotkey,hotkey-format}.ts`, `apps/tauri/src/Settings.tsx` (Shortcuts row "Switch mode", with clear), `apps/tauri/tests/fixtures/tauri-shim.ts`, `apps/tauri/scripts/pw-shim.js`, `apps/tauri/tests/settings-window.spec.ts`.

Outcomes:
- Both shells hold their slots in a list keyed by target; toggle and cancel behave as before (existing tests pass unchanged in meaning).
- An unset cycle slot installs nothing; a set one calls `modes::cycle` once per press.
- On macOS, the key of another modifier-tap slot, held before or pressed while a tap key is held, counts as "another key was pressed", so two modifier-tap slots cannot both fire. Any other modifier leaves the tap alone (hold Shift, tap Right Command: it fires, as before this task).
- A cycle binding that cannot be installed is reported in the hotkey status and dictation keeps working.
- The Shortcuts pane shows the third row, can capture, reset and clear it (Playwright).

Verification: shell `cargo test`, Windows target check, Playwright. Pressing the key in the Dev app is for the user.

## Task 4: Tauri commands, tray submenu, cycle feedback (TASK-96.4)

Files: `apps/tauri/src-tauri/src/modes.rs` (new), `apps/tauri/src-tauri/src/{lib,refine}.rs`, `apps/tauri/src-tauri/src/tray/mod.rs`, `apps/tauri/src/PillOverlay.tsx` (mode name shown briefly after a cycle).

`modes_get` (settings, the mode list with the built-in one, apps for a default), `modes_select`, `modes_cycle`; event `modes_changed` after every change, whoever made it.

Outcomes:
- The tray menu has a Mode submenu with the selected mode checked; choosing one selects it.
- A cycle (hotkey or command) emits `modes_changed` with the new mode, and the pill shows its name for a moment.
- The dictation tick carries the mode name, reason and app of the last dictation.

Verification: shell `cargo test`, Windows target check.

## Task 5: UI: Modes pane, Home select, mode on rows (TASK-96.5)

Files: `apps/tauri/src/lib/{modes,use-modes}.ts` (new, TanStack Query), `apps/tauri/src/components/{modes-pane,mode-dialog,home-switches,home-history,history-row,transcript-row,diagnostics-refine}.tsx`, `apps/tauri/src/lib/settings-panes.ts`, `apps/tauri/src/Settings.tsx`, `apps/tauri/src/components/sidebar-nav.tsx`.

Controls (shadcn, per `openwhisper-ui-discipline`): the mode list is rows with edit and delete revealed by the shared hover classes; a `Dialog` with `Input` (name), `Textarea` (instructions), `Select` (model, effort; first choice "Same as Refine"); `AlertDialog` to confirm a delete; per app a `Select` of modes with "No default"; on Home a `Select` (a fixed, short list that can grow, so not a toggle group).

Outcomes:
- Modes can be created, renamed, edited and deleted in Settings; Dictation shows as built in and has no delete.
- The Home select changes the selected mode and is disabled, with the reason, while Refine is off.
- An app from history can be given a default mode and lose it again.
- A dictation's row shows the mode that was used and why ("default for Slack").

Verification: `pnpm exec playwright test`.

## Task 6: Tests and verification (TASK-96.6)

Files: `apps/tauri/tests/modes.spec.ts` (new), `apps/tauri/tests/{home,history,settings-window}.spec.ts`, `apps/tauri/tests/fixtures/tauri-shim.ts`.

Outcomes:
- Core, CLI and shell cargo tests pass; Windows target check passes; the whole Playwright suite passes.
- Live eval numbers for a custom mode and the latency per mode are in the TASK-96 notes.
- TASK-96 is In Review with what is not verified listed (hotkey press, tray, pill label, dictating into an app with a default mode).

## Ordering

1 then 2 then 3 then 4 then 5 then 6. Tasks 3 to 5 share `lib.rs`, `Settings.tsx` and the test shim, so they run one after the other.

## Changes after review

- **A mode's model and effort belong to a backend.** `Mode.backend` is set when a model or effort is set (the backend Refine uses, or `--backend` in the CLI) and they apply only while that backend is the one in use; otherwise the mode runs with the Refine model and effort. A block from before the field existed is for the Claude CLI.
- **Mode names are unique**, ignoring case and outer space, and never "Dictation". `add` and `edit` refuse a taken name; `cleaned()` numbers duplicates edited in by hand ("Formal 2").
- **Short utterances in a custom mode** (five words or fewer) get 20 characters of slack over three times the input instead of 200 (`guard::check_rewrite`), so a preamble or a refusal is not typed in place of one or two words.
- **Settings writes** go through one process-wide lock and replace the file by rename.
- **The latest row names Dictation** when an app default put it over the selected mode; history entries still record custom modes only.
