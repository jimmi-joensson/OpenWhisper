---
id: doc-23
title: Crash reporting + in-app inspector — plan
type: plan
created_date: '2026-05-04 06:14'
---

**Backlog parent:** TASK-78
**Spec:** backlog/docs/specs/doc-22 - Crash-reporting-in-app-inspector.md

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

## Task 3: Diagnostics route wiring + crash list view

Render the inspector list under the existing top-level Diagnostics route. Implement the conditional sub-sidebar logic from the spec.

### Steps

1. Read the `openwhisper-ui-discipline` skill before touching `apps/tauri/src/components/`.
2. Sub-sidebar gating: introduce a static registry `apps/tauri/src/lib/diagnostics-panes.ts` (new) with one entry shape `{ id: string; label: string; Component: React.FC }`. TASK-78 ships a single `crashes` entry. TASK-62.8 (when it lands) appends an `overview` entry. The Diagnostics route renders a sub-sidebar **iff `DIAGNOSTICS_PANES.length >= 2`**; with one entry it renders that pane's component directly. This is the load-bearing rule — pure data, no module-resolution sniffing, no runtime feature detection.
3. Add `apps/tauri/src/components/crash-list.tsx` (new): newest-first list driven by `useEffect` polling `crashes_list` every 2s while the pane is visible. Justification: low enough for "fresh" feel after a panic-on-restart, high enough to avoid thrashing for a list that mutates only on user action or app launch.
4. Row design: shadcn `Card` or `<li>` with timestamp (relative + absolute via `Tooltip`), version, OS chip, truncated message, unread dot. Per-row `…` menu (shadcn `DropdownMenu`) with Mark-as-read, Delete (confirm via shadcn `AlertDialog`).
5. Empty state: copy "No crashes recorded." centered in a muted block. No emoji.

### Verification

- Manual: with no crash files present → empty state. Trigger a debug panic → restart → list shows one entry. Click "..." → Mark as read → unread dot disappears.
- Cross-check: while TASK-62.8 is To Do, Diagnostics renders crash list at root; once 62.8 lands, sub-sidebar appears with Overview + Crashes.

### Outcome ACs

- Crash list view renders newest-first with timestamp, version, OS, message, unread indicator
- Per-row Mark-as-read and Delete actions work and persist via the Task 2 commands
- Diagnostics shows sub-sidebar iff a sibling pane (TASK-62.8 Overview) is registered; otherwise renders single pane
- shadcn primitives used for menu + confirm dialog (per ui-discipline)

## Task 4: Detail view + Copy-as-markdown + Open-folder

Detail view with backtrace + events panes. Implement the markdown formatter and clipboard copy with redaction-aware output.

### Steps

1. New `apps/tauri/src/components/crash-detail.tsx`: header (timestamp/version/OS/phase/model), monospaced scrollable backtrace block, collapsible Events table (shadcn `Collapsible`).
2. Action row buttons: **Copy GitHub-ready report**, **Open crash folder**, **Delete**. Upload button stub (Task 6 fills in).
3. Markdown formatter in `apps/tauri/src/lib/crash-markdown.ts`: produces the format defined in the spec. Pure function, fully tested.
4. Vitest unit tests for the markdown formatter — fixed input crash file → exact expected markdown string. Catches accidental format drift.
5. **Open crash folder** invokes the `tauri-plugin-opener` plugin (already in `apps/tauri/src-tauri/Cargo.toml` as `tauri-plugin-opener = "2"`) — call `opener.openPath(<crashes-dir>)` from the webview. NOT `tauri::api::shell::open_path` (that's Tauri v1 API and isn't available on v2).
6. **Copy** uses `navigator.clipboard.writeText`; toast on success ("Copied. Paste into a GitHub Issue.").
7. The crash file already arrives redacted from Task 1 — the markdown formatter does not re-redact, but it MUST NOT add un-redacted fields (unit-test asserts against a fixture with PII-shaped strings).

### Verification

- Vitest formatter tests committed and green
- Manual: open a crash → click Copy → paste into a markdown preview → renders cleanly with collapsed backtrace
- Open-folder opens Finder/Explorer at the crashes dir on each platform

### Outcome ACs

- Detail view renders backtrace + events from a real crash file
- Copy button writes redacted markdown to clipboard in the spec-defined shape
- Markdown formatter has Vitest coverage for: full-shape, missing recording_state, empty events
- Open-folder works on macOS + Windows

## Task 5: Non-blocking launch notice + unread badge + bulk delete

Detect unread crashes at app boot and surface them without blocking. Add Delete-all to the list view.

### Steps

1. On Tauri ready, call `crashes_unread_count`. If ≥1, dispatch a Sonner toast (already in the project per shadcn norms — verify) with text "OpenWhisper recorded N crash report(s). View in Diagnostics." and an action "View" that routes to Diagnostics.
2. Sidebar nav badge: `apps/tauri/src/components/sidebar-nav.tsx` shows a small dot on the Diagnostics nav item while unread > 0. Persists across navigation. Clears only when the user **explicitly** marks each crash as read — never auto-dismissed on view. This keeps the badge a reliable backlog signal: "there are crashes you haven't acknowledged."
3. Delete-all button at top of crash list with shadcn `AlertDialog` confirm. Calls `crashes_delete_all`.
4. Unread count exposed for TASK-62.8 Overview pane consumption — add a tiny `<UnreadCrashesCounter />` component this task ships, which 62.8 imports if/when it lands.

### Verification

- Manual: trigger 2 debug panics, restart → toast shows "2 crash report(s)", badge dot on Diagnostics. Open list, click Mark all read on each → badge clears.
- Delete-all empties the dir and the list.

### Outcome ACs

- Launch toast appears non-blockingly when unread > 0; auto-dismisses; never blocks startup
- Sidebar Diagnostics nav shows unread dot until user marks crashes read
- Delete-all confirms then empties both files and state.json
- Unread-count component shipped and importable by TASK-62.8

## Task 6: Opt-in upload (per-crash button, configurable endpoint)

Wire the upload action behind explicit per-crash consent.

### Steps

1. Tauri command `crashes_upload(id: String) -> Result<()>` in `apps/tauri/src-tauri/src/crashes/`:
   - Resolve endpoint from env var `OPENWHISPER_CRASH_UPLOAD_URL` at command-time (NOT cached at boot — allows config change without restart).
   - If empty: return `Err("no endpoint configured")`.
   - HTTPS POST the JSON file body to the endpoint. Use `ureq` for the call. **Note:** `ureq` today lives only in `core/Cargo.toml` (gated optional feature for model download). It is NOT in `apps/tauri/src-tauri/Cargo.toml`. Add it to the tauri shell deps as `ureq = { version = "2", default-features = false, features = ["tls"] }` as part of this task. Single attempt, no queue.
   - On 2xx: write `state.json[id].uploaded_at = now`.
2. Detail view button **Upload to support endpoint**:
   - Disabled with tooltip if endpoint unset.
   - First-time confirm dialog (shadcn `AlertDialog`) listing exactly which fields are sent: backtrace, OS, app version, recording state. Quote the spec wording verbatim.
   - On success: button changes to "✓ Uploaded" and shows the relative timestamp. Idempotent — repeat upload allowed.
3. No retry, no auto-upload, no telemetry. Failure surfaces as a toast and the file stays on disk.

### Verification

- Manual against a local stub (e.g. `python -m http.server` with a small handler) — upload fires, status sets, idempotent retry works.
- Manual with no endpoint → button disabled with the right tooltip.

### Outcome ACs

- crashes_upload command exists and is the single upload path
- Button disabled state with explanatory tooltip when endpoint unconfigured
- First-upload confirm dialog lists the exact fields sent (matches spec wording)
- state.json records uploaded_at on success; failed upload does not delete the file

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

- Playwright spec for crash inspector committed and green; covers list, detail, copy, mark-read, delete-all
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
Task 6 (opt-in upload)
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
