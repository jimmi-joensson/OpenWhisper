---
id: doc-23
title: Crash reporting + in-app inspector — plan
type: plan
created_date: '2026-05-04 06:14'
---

**Backlog parent:** TASK-78
**Spec:** backlog/docs/specs/doc-22 - Crash-reporting-in-app-inspector.md

> **Design pivots — 2026-05-07.** Tasks 3, 4, 5, and 6 below have been rewritten in place to match the design handoff (see banner in `doc-22`). Tasks 1, 2, and 7 are unchanged. Companion: diagnostics-side polish (RSS breakdown bar in Memory section, Memory budget bar in Settings → Models, Storage panel) is planned separately under TASK-62 — see `backlog/docs/specs/doc-43 - Diagnostics-Models-design-polish-from-2026-05-07-handoff.md` + `backlog/docs/plans/doc-44 - Diagnostics-Models-design-polish-—-plan.md`.

## Overview

Seven implementation tasks, one commit each. Tasks 1–2 are core/IPC (Rust); 3–6 are shell/UI (TypeScript + Tauri); 7 is verification across both. Tasks 1 and 2 must land before any UI work — the UI is wired against the commands shaped in Task 2. Tasks 3–6 can interleave but the order below minimises rework.

Cross-task convention: every commit appends a one-liner to the matching subtask's notes via `--append-notes`. Subtask labels are `78-impl`.

## Task 1: Crash file schema + Rust panic hook

Install `std::panic::set_hook` in core's init path; serialize panics to versioned JSON in the OS-correct app log directory; capture redacted backtrace + recording-state snapshot + event ring buffer.

### Steps

1. Add `core/src/crashes.rs` (new): JSON schema types (`CrashFile`, `RustPanic`, `RecordingStateSnapshot`, `Event`), `redact()` helper, `write_crash_file(file: &CrashFile, dir: &Path) -> io::Result<PathBuf>`.
2. Add `core/src/crashes/event_buffer.rs` (new): bounded ring buffer (capacity 64), thread-safe (`Arc<Mutex<VecDeque<Event>>>`), `push_event(kind, data)` API. Drain into the crash file inside the hook.
3. Wire event-buffer pushes from existing dictation transitions in `core/src/dictation.rs` — instrument the points the state machine already crosses (idle → loading model, model loaded, recording start, transcribing, error). Touchpoints stay narrow per `openwhisper-orchestration-in-rust`.
4. In `apps/tauri/src-tauri/src/lib.rs` boot path (Tauri v2), resolve the crashes dir as follows: if env var `OPENWHISPER_CRASH_DIR_OVERRIDE` is set AND we are a debug or test build, use that path verbatim; otherwise call `app.path().app_log_dir()` (PathResolver method, NOT `tauri::path::app_log_dir()` — that's v1) and append `crashes/`. `create_dir_all`. Pass the resolved `PathBuf` into `core::crashes::install_panic_hook(dir, app_version)`. The override exists solely so Playwright (Task 7) can seed fixtures into a known temp dir; release builds ignore it entirely.
5. `install_panic_hook(dir, app_version)`:
   - Capture `panic_info.payload()`, thread name, location, `std::backtrace::Backtrace::force_capture()`
   - Read the dictation state via the process-wide `OnceLock<Arc<Mutex<DictationState>>>` registered at core init; `try_lock` and snapshot if held, else `null`
   - Drain event buffer
   - Compose `CrashFile { schema_version: 1, ... }`
   - Apply `redact()` to each `String` field (per spec: per-field, NOT against the serialized blob)
   - Write to `<dir>/<unix_ms>.json` with `serde_json::to_writer_pretty`
   - Chain the previous hook (`let prev = std::panic::take_hook(); ... prev(info);`) so Rust's default stderr message still prints — eprintln-style panic noise is preserved per spec non-goals
   - Best-effort: do not panic inside the hook; ignore IO errors silently
6. Unit test in `core/src/crashes.rs`: `redact()` strips `/Users/<name>/`, `C:\Users\<name>\`, env-var token patterns. Tests run as part of the standard `cargo test -p openwhisper_core` suite on macOS dev/CI; Windows is exercised in the Task 7 manual repro (we don't have Windows CI today; release-handover doc keeps Windows builds split across a second physical machine).
7. Unit test: a hand-built `CrashFile` round-trips through serde.

### Verification

- `cargo test -p openwhisper_core` green on macOS dev and macOS CI
- Manual on macOS: a debug-only `panic!()` in core triggers the hook, writes a file in the expected dir, file deserializes via `serde_json::from_str`
- Windows manual repro deferred to Task 7 (no Windows CI available)

### Outcome ACs

- Any panic on any thread produces a crash file at `<app_log_dir>/crashes/<unix-ms>.json`
- Crash file conforms to schema v1 (rust_panic, recording_state, events) and round-trips through serde
- Redactor strips home-dir paths and env-token patterns from every `String` field including backtrace; numeric/hashed fields untouched
- Recording-state snapshot uses try_lock against the registered `OnceLock<Arc<Mutex<DictationState>>>`; degrades to null if lock contended
- Default Rust panic stderr output still prints after the hook (chained, not replaced)
- Unit tests for redaction + serde round-trip committed and green

## Task 2: Tauri commands — list, read, delete, mark-read, debug-trigger

Surface the on-disk crash files to the UI through Tauri commands. Persist UI flags in a sibling `state.json`.

### Steps

1. New module `apps/tauri/src-tauri/src/crashes/mod.rs`. Six commands total:
   1. `crashes_list() -> Vec<CrashSummary>`: enumerate crash dir, deserialize each file, return summary (id, ts, app_version, os, message_truncated, unread).
   2. `crashes_read(id: String) -> CrashFile`: full file by id.
   3. `crashes_delete(id: String) -> Result<()>`: remove file + state entry.
   4. `crashes_delete_all() -> Result<()>`: nuke all files in dir + truncate state.json.
   5. `crashes_mark_read(id: String) -> Result<()>`: set unread=false in state.json.
   6. `crashes_unread_count() -> u32`: cheap counter for the launch notice + 62.8 panel.
2. `state.json` shape: `{ "entries": { "<id>": { "unread": bool, "uploaded_at": Option<unix_ms> } } }`. Hand-rolled JSON via serde, mirrors `apps/tauri/src-tauri/src/settings/mod.rs` style. State file is recreated if missing/corrupt.
3. Debug-only command `crashes_debug_trigger_panic()` behind `#[cfg(any(debug_assertions, feature = "dev-panic"))]`: panics on a tokio task, returns nothing. Used by Task 7.
4. Register all commands in the `tauri::Builder::default().invoke_handler(...)` list in `lib.rs`.

### Verification

- Smoke: write a crash file by hand in the dir, call `crashes_list` from a Tauri inspector, verify shape; call `crashes_mark_read`, verify state.json mutated.
- `crashes_debug_trigger_panic()` from a debug build — file appears, list returns it.

### Outcome ACs

- Six new Tauri commands wired and reachable from the webview
- state.json persists unread + uploaded_at flags; survives app restart
- Debug panic-trigger command exists and is gated to debug builds (no release exposure)
- List/read/delete are idempotent; delete-all is atomic per file (best-effort)

## Task 3: Diagnostics overview entry card + full-pane crash list

Wire the Crashes entry card on the existing Diagnostics overview pane (TASK-62.8 has shipped this pane in In Review state) and the full-pane crash list reachable from it. **No sub-sidebar.** This task replaces the v1 sub-sidebar gating draft.

### Steps

1. Read the `openwhisper-ui-discipline` skill before touching `apps/tauri/src/components/`.
2. **Diagnostics overview — Crashes entry card.** In `apps/tauri/src/components/diagnostics-pane.tsx`, add a `Crashes` section below the existing `Memory` section using the same `<section>` + mono kicker pattern. The section body is a single button-styled card (full-width, sunken background, rounded 8) containing: a 28×28 destructive-tint tile with the crash glyph, the label "Crash reports" + recording-orange unread pill (`<count> unread`), a mono "Last: <relative> · <module> · <signal>" sub-line, and a chevron on the right. Polls `crashes_unread_count` + a new `crashes_summary` (returns the latest one row) every 2 s while the overview is visible. When `unread === 0 && total === 0`, the card is replaced by a one-line muted "No crashes recorded · Open crash folder" link instead of being hidden — silence is louder than a missing affordance.
3. **Route + state.** Extend the Diagnostics route's local view-state with `view: "overview" | "crashes"` (component-local React state, not a router-level route). Tapping the entry card sets `view: "crashes"`. Browser/app-level back navigation (Esc on overview, sidebar nav) is not affected.
4. **Crash list pane.** Add `apps/tauri/src/components/crash-list.tsx` (new). Layout: pane header (breadcrumb-style `← Diagnostics` ghost button + "/" mono separator + "Crashes" mono kicker + `<unread> unread · <total> total` text on left; `Delete all` destructive-ghost button on right) — then a full-width scrollable list. List polls `crashes_list` every 2 s while visible.
5. **Row component.** `apps/tauri/src/components/crash-row.tsx` (new). Three-column grid: 20px unread-dot column (recording-orange dot + halo when `unread`, transparent otherwise) · body column · 100px-min actions column. Body shows three lines: line 1 — relative timestamp (with absolute via shadcn `Tooltip`) + mono version + mono OS chip + optional `uploaded` mono pill; line 2 — mono one-line cause (`message`, `text-overflow: ellipsis`); line 3 — mono `phase: <X> · model: <Y>`. Click on the row body opens the sheet (Task 4); click on the action buttons must `stopPropagation`. **Hover state reveals two icon buttons** — `[✓] Mark as read` (only if `unread`) and `[🗑] Delete`. **No `…` overflow menu.** **Single-row delete is one-click — no AlertDialog.** Resting state shows only a chevron.
6. **Mark-as-read paths.** Both row click (which opens the sheet) AND the per-row `[✓]` invoke `crashes_mark_read(id)`. Closing the sheet does NOT un-read.
7. **Empty state.** When `crashes.length === 0`, render the empty composition full-pane (44px crash glyph in a muted tile, "No crashes recorded" h2, "We log crashes to <code>~/Library/Logs/OpenWhisper/crashes/</code> so you can read or delete them yourself." caption, single "Open crash folder" ghost button calling `tauri-plugin-opener`). The pane header (breadcrumb + Delete-all) is hidden in empty state.
8. **Delete-all** dialog uses shadcn `AlertDialog` (title "Delete all crash reports?", body "<unread> unread will be removed too. This can't be undone.", confirm label "Delete <total> reports").

### Verification

- Playwright (extends `apps/tauri/tests/crash-inspector.spec.ts` from Task 7): seed two crashes → assert overview shows entry card with "2 unread" pill → click card → assert URL view-state advances to `crashes` → assert two list rows render → hover first row → assert mark-read + delete buttons visible.
- Manual: trigger debug panic → restart → click entry card → list renders → click row → sheet opens (Task 4 verifies sheet contents).
- ui-discipline check: every primitive is shadcn (Button, AlertDialog, Tooltip) or composed from shadcn — no styled `<div>` reaching for primitive duties.

### Outcome ACs

- Diagnostics overview pane renders a Crashes entry card with live unread pill + last-crash summary, polled at 2 Hz
- Tapping the card swaps the Diagnostics pane to the crash list (no sub-sidebar, no nested rail), with a `Diagnostics /` breadcrumb back to overview
- Crash list renders rows with hover-revealed `[✓]` mark-read + `[🗑]` delete; resting row shows chevron only; row click opens the detail sheet AND marks the crash read
- Single-row Delete is one-click (no confirm dialog); Delete-all uses shadcn AlertDialog with "<unread> will be removed" body
- Empty state replaces the entire pane with the empty composition and a single Open-crash-folder button; pane header is hidden in this state
- shadcn primitives used for AlertDialog + Tooltip + Button (per ui-discipline)

## Task 4: Detail sheet + Copy-as-markdown + Open-folder

Render the detail view inside a right-side sheet that overlays the list. Implement the markdown formatter and clipboard copy with redaction-aware output.

### Steps

1. **Sheet primitive.** Use shadcn `Sheet` with `side="right"`, custom width 580 px (max 85% viewport). The list stays mounted behind a 40%-black + slight-blur backdrop. Esc / backdrop-click / header-`✕` closes the sheet. Mark-read fires once on `onOpen` (i.e. when `openId` becomes non-null) — closing does not un-read.
2. **`apps/tauri/src/components/crash-detail.tsx`** (new). Three vertical regions:
   - **Sticky header**: mono kicker `Crash report` on the left, `✕` close button on the right, divider below.
   - **Scrollable body**: identity block (panic message in mono 15px, then a 3-line meta strip — absolute timestamp + version/build, OS + arch, mono `phase: … · model: … · session …`); `Backtrace` block in a sunken card (max-height 220 px internal scroll, mono 11.5px, columns: index · module · address · symbol); collapsible `Events (N)` block — when expanded, shadcn `Collapsible` reveals a table with sticky 4-col header (Time / Phase / Event / Detail) and a max-height-180 internal scroll. The crash event row is left-bordered in `--recording`.
   - **Sticky footer** (always visible regardless of body scroll position):
     - Primary: `Copy GitHub-ready report` filled button. Click → label flips to `✓ Copied` for 1.2 s. Right-aligned `⌘C` mono hint.
     - Secondary row: `Open crash folder` ghost · `Report on GitHub` ghost (Task 6 wires the URL builder + opener call) · spacer · `Delete` destructive-ghost.
3. **Delete-from-sheet** calls `crashes_delete(id)` and closes the sheet (returns to a one-shorter list). No confirm.
4. **Open crash folder** invokes the `tauri-plugin-opener` plugin (already in `apps/tauri/src-tauri/Cargo.toml`) — `opener.openPath(<crashes-dir>)` from the webview. NOT `tauri::api::shell::open_path` (Tauri v1 only).
5. **Markdown formatter** in `apps/tauri/src/lib/crash-markdown.ts` (new). Pure function `formatCrashAsMarkdown(crashFile: CrashFile): string` producing the format defined in the spec.
6. **Copy** uses `navigator.clipboard.writeText` directly from the button handler; the inline `✓ Copied` swap is the success surface (no separate toast — keep the user's eyes on the sheet).
7. **Vitest unit tests** for the markdown formatter: fixed input → exact expected string for full-shape, missing `recording_state`, empty `events`. Plus a redaction-aware regression: fixture with PII-shaped strings → output must NOT contain raw `/Users/<name>/`, env-var-token-pattern values, or `C:\Users\<name>\` paths (the writer at Task 1 handles redaction; this asserts the formatter doesn't re-introduce them).

### Verification

- Playwright (extends crash-inspector spec): click row → sheet slides in from right → assert backdrop visible + list dimmed but still rendered → assert sticky footer's `Copy GitHub-ready report` is visible without scrolling, even when the backtrace is scrolled to the bottom → click Copy → assert clipboard matches a markdown fixture → close via `✕` → assert sheet animates out and row is now in read state.
- Vitest unit tests for `crash-markdown.ts` committed and green.
- Manual: paste copied markdown into a GitHub Issue draft → renders cleanly with collapsed backtrace.

### Outcome ACs

- Detail renders inside a right-side sheet (~580 px) over a dimmed backdrop; closing returns to the (now-read) list
- Sheet has sticky header + sticky action footer; backtrace scroll never hides the primary `Copy` button
- Opening the sheet marks the crash read; closing the sheet does not un-read; deleting from the sheet closes it and removes the row
- `Copy GitHub-ready report` writes redacted markdown to the clipboard and inline-flips to `✓ Copied` for 1.2 s
- Markdown formatter has Vitest coverage for: full-shape, missing `recording_state`, empty `events`, and a PII-shaped redaction-regression fixture
- Open-crash-folder works on macOS + Windows via `tauri-plugin-opener`

## Task 5: Delta-driven launch toast + persistent rail dot + bulk delete

Detect unread crashes at app boot and surface them without blocking, with a strict separation of toast (transient, delta-driven) from rail dot (persistent, mark-read-driven). Add Delete-all to the list view.

### Steps

1. **`lastSeenUnreadCount` in settings.** Extend `apps/tauri/src-tauri/src/settings/mod.rs` schema with a new u32 field `last_seen_unread_count` (default 0). Persisted alongside other settings. Updated by the shell on each `crashes_mark_read`, `crashes_delete`, `crashes_delete_all`, and on a successful "Dismiss" of the launch toast — but NOT on toast auto-dismiss (auto-dismiss without user interaction must not consume the delta signal).
2. **Toast (delta-only).** On Tauri ready, call `crashes_unread_count` AND read `last_seen_unread_count`. If `currentUnread > lastSeen`, dispatch a Sonner toast (verify Sonner is in the project; fall back to the existing primitive if not). Copy varies by phase available from the latest unread crash: "OpenWhisper crashed during recording." (recording phase) or "OpenWhisper crashed last session." (other phases). Sub-line "Diagnostics has the report." Buttons: `View` (route to Diagnostics overview — `setView("home → diagnostics")`, NOT direct into the inspector) and `Dismiss` (closes toast and updates `last_seen_unread_count = currentUnread`). Auto-dismiss 8 s, hover-to-pause. Clicking `View` does NOT update `last_seen_unread_count` — only opening a specific crash counts.
3. **Rail dot.** `apps/tauri/src/components/sidebar-nav.tsx` shows a 6 px recording-orange dot with a 25%-tint halo on the Diagnostics nav item while `unread > 0`. Driven by the same `crashes_unread_count` poll. Cleared only when each crash is explicitly marked read (via row-click open or per-row `[✓]`). Never auto-dismissed by visiting the route.
4. **Delete-all.** AlertDialog (shadcn) wired to `crashes_delete_all`. Confirm-button label is `Delete <total> reports`. Body copy: "<unread> unread will be removed too. This can't be undone."
5. **Crashes-summary command.** Add `crashes_summary() -> Option<{ when_relative: String, module: String, signal: String }>` returning a tiny struct for the Diagnostics overview entry card's sub-line ("Last: 2 days ago · recognizer.so · SIGSEGV"). Cheaper than `crashes_list()` for that surface.

### Verification

- Playwright: seed 2 crashes → app start → assert toast appears with "during recording" copy → assert rail dot visible → click `Dismiss` → toast gone, dot stays. Restart with same 2 still unread → assert toast does NOT appear (no delta) → assert dot still visible. Open one crash → close sheet → restart → assert toast still does NOT appear (currentUnread=1, lastSeen=2, no positive delta).
- Manual: trigger 1 debug panic → restart → toast appears once → close sheet for that crash → assert dot clears and `last_seen_unread_count` settles to 0.
- Delete-all: confirm dialog → confirm → list empty → state.json truncated to `{ "entries": {} }`.

### Outcome ACs

- Settings schema gains `last_seen_unread_count: u32` (default 0), persisted across restart
- Launch toast fires only when `currentUnread > lastSeenUnread`; subsequent restarts at same/lower unread show only the rail dot
- Rail dot persists until each unread crash is explicitly marked read; never auto-dismissed by route visits, time-out, or toast dismiss
- `View` button routes to Diagnostics overview (not the inspector) — entering the inspector requires the user's explicit click on the Crashes entry card, which is the read action
- Delete-all empties both crash files and `state.json` entries; confirm dialog uses dynamic count copy
- `crashes_summary` command exists and returns the latest crash's relative-when + module + signal for the entry card's sub-line

## Task 6: Report on GitHub button (rescoped from opt-in upload — 2026-05-07)

**Rescope rationale.** The v1 design (HTTPS POST to a configurable endpoint behind an Includes/Excludes disclosure dialog with per-endpoint suppress) assumed a multi-collector world OpenWhisper doesn't actually live in. The bug tracker is the GitHub Issues queue at `jimmi-joensson/OpenWhisper`. Building + maintaining a generic upload endpoint (which, in practice, only the maintainer would ever configure) isn't worth the surface area. Replaced with a single button that opens a prefilled GitHub issue — same user benefit (one-click crash report submission), zero infrastructure, no settings schema bump, no dialog-stack-on-sheet.

The original design lives in git history (commit predating this rescope) and in `backlog/docs/specs/doc-22 - Crash-reporting-in-app-inspector.md` under the new `Report on GitHub` section.

### Steps

1. **URL builder.** `apps/tauri/src/lib/crash-github.ts` (new): pure function `buildGitHubIssueUrl(crash: CrashFile, opts: { owner: string; repo: string; appVersion: string }): string`. Composes:
   - `title = "Crash report — v<appVersion> — <truncated panic message ≤72 chars>"`
   - `body = formatCrashAsMarkdown(crash)` (re-uses 78.4's formatter — single source of truth)
   - `labels = "bug,crash"`
   - URL: `https://github.com/<owner>/<repo>/issues/new?title=…&body=…&labels=…`
   - URL-encode title + body via `encodeURIComponent`.
   - **Truncate body** when over 6 KB: keep the identity block in full, trim the backtrace tail, append `\n\n_Truncated — paste the full report from \`Copy GitHub-ready report\` if needed._`. Pure function; deterministic.
2. **Sheet footer button.** `crash-detail-sheet.tsx`: replace the `UploadAffordance` placeholder with a `Report on GitHub` ghost button alongside `Open crash folder`. Click handler invokes `tauri-plugin-opener`'s `openUrl` with the built URL.
3. **CLI parity.** Extend `cli/src/commands/crash_dump.rs` with a `--github-url` flag: prints the URL to stdout (plain text or `{ "url": ... }` under `--json`). Honors `--latest` / `--id <ID>` the same way as the existing print modes. Lets a power user run `openwhisper crash-dump --github-url --id 1717503600123 | xargs open` from a shell.
4. **Vitest** for `buildGitHubIssueUrl`: title composition + truncation, label list, encoding round-trip, identity-block-preserved truncation.
5. **Playwright** for the button: open the sheet, assert `Report on GitHub` is visible, click → assert the opener was invoked with a URL matching `^https://github\.com/jimmi-joensson/OpenWhisper/issues/new\?` and the body contains the expected markdown shape.

### Verification

- `pnpm test:unit` green for `crash-github.test.ts`.
- `pnpm test:ui tests/crash-inspector.spec.ts` green for the new "Report on GitHub" case.
- `cargo build -p openwhisper-cli` clean.
- `openwhisper crash-dump --github-url --id <ID>` prints the URL; `--json` round-trips through `jq -r .url`.
- Manual: open a real crash → click Report on GitHub → browser opens prefilled issue → submit (or dismiss) → exit cleanly.

### Outcome ACs

- Sheet footer renders a `Report on GitHub` ghost button next to `Open crash folder` (replacing the 78.4-era `Upload` placeholder).
- Clicking the button opens `https://github.com/jimmi-joensson/OpenWhisper/issues/new` with prefilled title, body, and `labels=bug,crash` via the platform default browser.
- Body is the same redacted markdown `formatCrashAsMarkdown` produces — single source of truth, no separate formatter.
- Body is truncated to fit GitHub's URL length cap (~6 KB) with a trailing _"Truncated — use Copy GitHub-ready report for the full body"_ marker; full Copy flow stays available as the fallback.
- `openwhisper crash-dump --github-url` prints the URL for the latest crash (or `--id <ID>`); honors `--json`.
- Vitest covers the URL builder; Playwright covers the button click flow.

## Task 7: Manual repro + Playwright + redaction regression

End-to-end verification across both platforms; lock in the format with a regression test.

### Steps

1. Playwright spec `apps/tauri/tests/crash-inspector.spec.ts`:
   - **Log dir override:** the Tauri shell honours an env var `OPENWHISPER_CRASH_DIR_OVERRIDE` (added in Task 1 alongside the `app_log_dir()` resolve, gated to debug + test builds). When set, the panic hook and all command paths use that path instead of `app_log_dir()/crashes/`. The Playwright spec sets it to a temp dir before launching the app.
   - Seed two crash JSON files in that temp dir from a fixture using Node `fs.writeFileSync` from the test setup hook — no Tauri test-harness magic required.
   - Start app → assert toast appears with "2 crash report(s)" → assert Diagnostics badge dot.
   - Navigate to Diagnostics → assert two list rows in expected order (newest first) → click first → assert detail view headers + backtrace pane visible.
   - Click Copy → read clipboard → assert it matches the expected markdown fixture.
   - Click Mark as read → assert badge dot clears after both rows handled.
   - Click Delete-all (confirm) → assert empty state.
2. Manual repro on macOS:
   - Run debug build → `crashes_debug_trigger_panic` → restart → confirm toast/file/list/copy.
3. Manual repro on Windows:
   - Same flow.
4. Redaction regression: hand-build a fixture crash file that contains `/Users/jimmijoensson/secret`, an `OPENAI_API_KEY=sk-...` line, and a `C:\Users\Bob\Desktop\notes.txt` path. Assert the copy markdown contains `<HOME>` / `<redacted>` and none of the source PII.
5. Update `apps/tauri/CLAUDE.md` (or add to it) noting the crash-inspector test file as part of the "verifying changes" loop required by project CLAUDE.md.

### Verification

- `pnpm test:ui` (Playwright suite) passes locally on macOS and on the Windows machine
- Fixture-based redaction assert runs in CI on every PR touching `core/src/crashes.rs` or `apps/tauri/src/lib/crash-markdown.ts`

### Outcome ACs

- Playwright spec for crash inspector committed and green; covers Diagnostics overview Crashes entry card, full-pane list, right-side detail sheet open/close, mark-read on sheet open, delta-driven launch toast (fires once, suppressed on no-delta restart), per-endpoint upload-dialog suppress checkbox persistence, single-row delete (no confirm), Delete-all confirm dialog, empty-state composition
- Manual repro confirmed on macOS + Windows: panic → file → next-launch toast → list → copy → paste-clean markdown
- Redaction regression test rejects PII-shaped strings in clipboard output
- Project CLAUDE.md / apps CLAUDE.md updated to point at the new spec file

## Sequencing & dependencies

```
Task 1 (core panic hook + schema)
   ↓
Task 2 (Tauri commands)
   ↓
Task 3 (list view + Diagnostics route)
   ↓
Task 4 (detail view + copy)
   ↓
Task 5 (launch notice + bulk delete)
   ↓
Task 6 (Report on GitHub button — rescoped 2026-05-07)
   ↓
Task 7 (Playwright + cross-platform manual repro)
```

Strictly linear. Tasks 3–6 each depend on the surface from the previous (e.g. Task 5 needs the list page from Task 3). Task 7 must close last and exercises everything end-to-end.

## Cross-plan dependencies

- **TASK-62.8** (Diagnostics Overview pane) — coordination, not blocking. The conditional sub-sidebar logic in Task 3 handles whichever order they ship.
- **TASK-79** (level-stream stall fix) — should land before Task 5 ships, otherwise the toast competes with stutter for "what's the worst thing about the app right now?" attention. Not a hard blocker.
- **TASK-10** (custom vocab post-processing) — unrelated, but the missing-words investigation in TASK-80 may surface crashes worth correlating; the schema's `events` ring buffer makes this possible.

## Open knobs deferred to implementation

Genuinely small decisions, not load-bearing:

- Toast library: Sonner if already in the project, otherwise the project's existing toast primitive — verified during Task 5
- Crash-folder open: `tauri::api::shell::open_path` vs `dialog` — function call site in Task 4
- Polling interval for crash list: 2s default; raise if it shows up in profiling (probably won't)

Anything outside this list comes back to the spec for resolution.
