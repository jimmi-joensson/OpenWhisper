---
id: doc-22
title: Crash reporting + in-app inspector
type: spec
created_date: '2026-05-04 06:12'
---

**Backlog parent:** TASK-78

> **Design pivots — 2026-05-07.** A design-tool handoff (chat7 + chat6 transcripts in the bundle) revised the UX in five load-bearing places: (1) **no sub-sidebar** — Crashes reached as a section card on the Diagnostics overview, full-pane swap; (2) **detail in a right-side sheet** over the dimmed list, with sticky action footer; (3) **mark-read on open** + hover-revealed `[✓]` / `[🗑]` per row, no `…` menu; (4) **single-row delete = no confirm** (Delete-all keeps confirm); (5) **upload dialog** carries an explicit Includes/Excludes block + "Don't ask again for this endpoint" checkbox, and a successful upload swaps the button for a mono `Uploaded · just now` label. Sections below have been rewritten in place; original draft preserved in git history (commit predating this banner).

## Problem

A Rust panic in OpenWhisper's core or Tauri shell is currently lost: no backtrace, no record on disk, no way for the user to forward it. We saw this concretely once on Windows — a long recording crashed and the only signal was the user's memory of the event. With ~zero crash signal we can't diagnose anything that isn't reproduced live in front of us.

OpenWhisper is open-source and most reports come in as GitHub Issues. The most valuable thing we can give a user is a crash report they can paste into an issue verbatim, with no shell hunting and no manual editing.

## Goals (v1)

- Capture every Rust panic that reaches the panic hook with backtrace + minimal context, written to a versioned JSON file in the OS-correct app log directory.
- Show captured crashes inside the app under a Diagnostics route. List + detail view, no modal interrupts.
- One-click "Copy GitHub-ready report" producing redacted markdown ready to paste into an Issue.
- One-click "Report on GitHub" — opens a prefilled GitHub issue at `jimmi-joensson/OpenWhisper` with the same redacted markdown the Copy flow produces. No upload endpoint, no settings schema bump, no infrastructure (rescoped 2026-05-07; the original "configurable upload endpoint" design lives in git history + the Report-on-GitHub section below).
- Cross-platform: macOS + Windows. Same file format, same UI, OS-native log dir.

## Non-goals (v1)

- No Sentry / Crashpad / Breakpad SDK. Re-evaluate once we have real crash volume worth aggregating.
- No webview JS error capture. Rust panics only. (Browser-side errors can be added later if they prove valuable; the file format leaves room.)
- No automatic upload, no telemetry, no anonymous "did you crash?" pings. Strictly user-initiated.
- No log-tail of structured events: the codebase currently uses `eprintln!` everywhere with no `tracing` / `log` crate (confirmed 2026-05-04). Adding a logging facility is a separate concern; v1 ships without a "last N log lines" field. We instead capture a small in-process **event ring buffer** (see schema) that records key dictation state transitions — sufficient context without rewriring logging across the codebase.
- No crash-on-crash safety net (a crash inside the panic hook itself). v1 best-effort: serialize, flush, exit. Acceptable risk.
- The panic hook does not replace stderr output. Existing `eprintln!`-style panic noise still reaches stderr after the crash file is written — Terminal users see something, file ingestion is additive. (Implementation note: chain after the previous hook via `let prev = std::panic::take_hook();` then call `prev(info)` at the end of our hook so Rust's default formatted message still prints.)

## Behaviour model

### Storage location

| OS | Path |
|---|---|
| macOS | `~/Library/Logs/<bundle-id>/crashes/<unix-ms>.json` |
| Windows | `%LOCALAPPDATA%\<bundle-id>\logs\crashes\<unix-ms>.json` |

`<bundle-id>` is `com.openwhisper.dev` in dev builds and `com.openwhisper.app` in release builds; the path is resolved at runtime by `app.path().app_log_dir()` (Tauri v2 PathResolver API), never hard-coded.

Rationale: crashes are diagnostic artifacts, not user data. Tauri's `app_log_dir()` already follows the platform conventions Apple/Microsoft expect, and matches how `app_config_dir()` is used today for `settings.json` (`apps/tauri/src-tauri/src/settings/mod.rs:222`). The earlier draft's `~/.openwhisper/crashes/` is wrong — would be the only OpenWhisper artifact at that path on either OS.

A sibling `state.json` in the same dir tracks per-crash UI flags (`unread`, `uploaded_at`) — keeps the immutable crash file untouched while the inspector tracks user actions.

### Crash file schema (v1)

```jsonc
{
  "schema_version": 1,
  "id": "1717503600123",            // unix-ms; same as filename
  "ts_unix_ms": 1717503600123,
  "app_version": "0.6.0",            // matches Cargo package version + Tauri identifier
  "os": "macOS 15.4 (arm64)",        // free-form String; examples: "macOS 15.4 (arm64)", "Windows 11 (x86_64)"
  "rust_panic": {
    "thread_name": "tokio-runtime-worker",
    "message": "called `Result::unwrap()` on an `Err` value: ...",
    "location": "core/src/audio.rs:412:17",
    "backtrace": "<full std::backtrace::Backtrace::force_capture string, redacted>"
  },
  "recording_state": {               // null if crash was outside a dictation
    "status_message_at_crash": "transcribing on ANE…",  // verbatim from DictationState.status_message (core/src/dictation.rs:59) — String, NOT a typed enum (no Phase enum exists in core today)
    "duration_ms": 18234,
    "device_id_hash": "sha256:8 hex chars",   // hashed, never raw device label
    "model_kind": "Parakeet" | "FluidAudio",
    "samples_captured": 291744
  },
  "events": [                         // ring buffer, oldest first, max 64
    { "ts_unix_ms": ..., "kind": "DictationStart" | "PhaseChange" | "ModelLoaded" | "DeviceChanged" | "Error", "data": { /* small struct, no PII */ } }
  ]
}
```

Schema is versioned for forward-compat. Reader treats unknown top-level fields as additive.

### Redaction (applied at write time, before serialization)

Redaction operates **per `String` field** in the typed `CrashFile` struct (driven by the writer iterating named string fields, e.g. `rust_panic.message`, `rust_panic.location`, `rust_panic.backtrace`, each `events[].data` string value, `recording_state.status_message_at_crash`). It does NOT scrub the serialized JSON blob — that would risk eating field names that happen to match a path pattern. Numeric and hashed fields (`device_id_hash`, sample counts, timestamps) are skipped.

The redactor applies these rules to each input string:

- `/Users/<name>/...` → `/Users/<redacted>/...`
- `C:\Users\<name>\...` → `C:\Users\<redacted>\...` (and the `\\?\` and forward-slash variants)
- Any path component matching the user's home dir (looked up via `dirs::home_dir()`) → `<HOME>`
- Substrings that **look like an env-var assignment in text** — i.e. patterns of the form `(AWS_[A-Z_]+|OPENAI_[A-Z_]+|ANTHROPIC_[A-Z_]+|[A-Z_]+_TOKEN|[A-Z_]+_KEY)=<value>` appearing inside a string (typically a backtrace or panic message that captured them). Replace `<value>` with `<redacted>`, keep the key. **Important:** the panic hook does NOT snapshot the process's environment variables and does not embed env in the crash file — this rule scans backtrace/message text only, defensively, in case a panic message accidentally formatted a secret.

Out of scope for redaction: backtrace symbol names (which include crate paths like `openwhisper_core::audio::process_chunk`). These are necessary signal and contain no PII.

The same redactor runs against the markdown produced by "Copy GitHub-ready report" — clipboard is not a back door. Because the on-disk file is already redacted at write time, the markdown formatter does not re-redact; it simply must not introduce un-redacted fields (asserted in unit tests).

### Recording-state snapshot

Captured by reading the dictation state machine from the panic hook. State lives in core per `openwhisper-orchestration-in-rust`. Concretely the hook reaches it via a process-wide `static DICTATION_STATE: OnceLock<Arc<Mutex<DictationState>>>` registered during core init (the same `Arc<Mutex<…>>` the existing IPC commands borrow through their `tauri::State<…>` wrapper — registering the underlying `Arc` in a `OnceLock` lets the panic hook read it without going through the Tauri command machinery, which is unavailable from arbitrary panicking threads). If no dictation is active, the snapshot field is `null`. The snapshot uses `try_lock` — if the state lock is held by the panicking thread, the snapshot is `null` rather than risking a deadlock inside the hook.

### Panic hook scope

- Rust panics from any thread reaching `std::panic::set_hook` are captured. Set the hook in `core::lib::init()` (or equivalent entry) so it runs for both the Tauri shell and the core.
- Tokio task panics propagate to the hook because `catch_unwind` is not used in our task spawns; the runtime's default behavior aborts the process, the hook fires before exit. Verify in the manual repro task.
- Webview JS errors are out of scope (no panic hook reach there). Listed as non-goal.

### UI architecture

The crash inspector lives under the existing top-level **Diagnostics** route in `apps/tauri/src/components/sidebar-nav.tsx` (`Route = "home" | "settings" | "diagnostics"`). It is **not** a Settings sub-pane — Settings stays config-only.

**Single rail, no sub-sidebar.** The original draft proposed a Diagnostics sub-sidebar (Overview / Crashes) gated on TASK-62.8. The 2026-05-07 design pass dropped that. Crashes is reached as a **section card on the Diagnostics overview pane** (the same `CrashesSection` row already mocked in `diagnostics.jsx`), with chevron + unread-count badge. Tapping the card swaps the pane content to the full-pane crash inspector — same Diagnostics route, no second-level sidebar — with a `Diagnostics /` breadcrumb-style back link in the inspector header. Sub-sidebar conditional logic (`DIAGNOSTICS_PANES.length >= 2`) is **not** shipped.

Rationale: the user explicitly wanted "no multiple levels of sidebars." The card-as-entry pattern is consistent with the Memory and Performance section cards on the same overview, and lets us add future Diagnostics sub-routes (perf detail, recognizer trace) without ever introducing a nested rail.

### List view

- Newest-first, **full-pane** list across the entire Diagnostics content area (no left-rail second column).
- Pane header: breadcrumb (`← Diagnostics /` + "Crashes" label) on the left, `unread N · total M` count text in the middle, **Delete all** button on the right.
- Row layout (single row, three columns): unread dot · body · actions.
  - Body: timestamp ("2 days ago"), app version + OS chip on the same line; one-line cause (`rust_panic.message` truncated) on a second line; `phase: <X> · model: <Y>` mono meta on a third line.
  - Actions column on rest: chevron only.
  - Actions column on hover: small `[✓]` Mark-as-read button (only when `unread`) + `[🗑]` Delete button. **No `…` overflow menu.**
- **Single-row Delete is one-click — no confirm dialog.** (The undo cost is low: the file is just gone from the inspector list. The on-disk JSON is what matters; if the user deletes the wrong row they re-trigger from a fresh repro. Confirm is reserved for Delete-all.)
- **Selection marks read.** Clicking a row body (anywhere outside the action buttons) opens the detail sheet AND clears `unread` for that crash. Per-row `[✓]` remains as the "I've triaged this without opening it" affordance.
- Empty state: list rail collapses entirely; the empty composition fills the pane (44px crash glyph in a muted tile, "No crashes recorded" headline, "We log crashes to `~/Library/Logs/OpenWhisper/crashes/` so you can read or delete them yourself." caption, and a single "Open crash folder" button).

### Detail view

Detail is rendered inside a **right-side sheet (slide-over)**, ~580 px wide, layered over a dimmed (40% black + slight blur) backdrop covering the list. The list stays mounted behind the backdrop so the user keeps spatial context. Closing the sheet (header `✕`, backdrop click, or Esc) returns to the list with the row now in the read state.

Sheet layout:

- **Sticky header**: `Crash report` mono kicker on the left, `✕` close button on the right. Single-line, divider below.
- **Scrollable body**:
  - Identity block: full panic message (mono, 15px), then a 2–3-line meta strip (absolute timestamp, app version + build, OS + arch, phase/model/session-length).
  - Backtrace block: `Backtrace` mono kicker + `Copy backtrace` ghost button, then a sunken card with the monospace stack frames (max-height 220 px, internal scroll). No syntax highlighter.
  - Events block: collapsible (`▸ Events (N)`), opens to a sticky-header table with Time / Phase / Event / Detail columns. The crash event row is left-bordered in recording-orange.
- **Sticky action footer** (always visible at sheet bottom; doesn't scroll out from under the user when they're paging the backtrace):
  - **Primary**: `Copy GitHub-ready report` button. After click → label flips to `✓ Copied` for ~1.2s. `⌘C` hint sits on the right.
  - **Secondary row**: `Open crash folder` ghost · `Upload` ghost (if endpoint configured AND not yet uploaded) OR `Uploaded · just now` mono label (if already uploaded; no enabled re-upload affordance — re-uploading the same crash is not a useful action) · spacer · `Delete` destructive-ghost.
- **Mark-as-read on open.** Opening the sheet IS the read action; closing the sheet does NOT un-read.
- **Delete inside the sheet** closes the sheet and returns to the (now-shorter) list.
- **Upload AlertDialog stacks above the sheet**, doesn't replace it — the list, sheet backdrop, and sheet are all dimmed under the dialog's own backdrop.

### Copy-as-markdown format

```markdown
**OpenWhisper crash report**

- Version: 0.6.0
- OS: macOS 15.4 (arm64)
- When: 2026-05-04 14:33:21 UTC
- Phase at crash: Recording (18.2s in)
- Model: Parakeet (CoreML)

**What I was doing:**

> _replace this with a quick description before submitting_

<details>
<summary>Backtrace (click to expand)</summary>

```
<panic message>
   at core/src/audio.rs:412:17

<full redacted backtrace>
```

</details>

<details>
<summary>Recent events</summary>

| time | kind | data |
| --- | --- | --- |
| ... | DictationStart | ... |

</details>
```

Format renders cleanly as a GitHub Issue body. The `What I was doing` block is the ONE thing the user has to fill in before posting — the rest is mechanical.

### Non-blocking launch notice

On app start, two indicators run in parallel — and they have different lifecycles:

**Diagnostics rail dot** (sidebar badge — `apps/tauri/src/components/sidebar-nav.tsx`):
- Lives on the top-level Diagnostics nav item. There is no second-level sidebar to dot.
- Visible while `unread > 0`. **Cleared only when each unread crash is explicitly marked read** (by opening its sheet or clicking the per-row `[✓]`).
- Never auto-dismissed by visiting Diagnostics, by time, or by closing the toast. Visiting the route is not the same as reading a specific crash.

**Launch toast** (Sonner — verify at impl time, fall back to project's existing primitive):
- Fires **only on the run that introduced the new unread**, i.e. when `currentUnread > lastSeenUnreadCount`. We persist `lastSeenUnreadCount` in settings on app shutdown / on each mark-read. Subsequent launches with unread but no delta show only the rail dot, not a fresh toast.
- Copy mentions the phase if available: "OpenWhisper crashed during recording." (or "…last session." for non-recording phases). Sub-line: "Diagnostics has the report."
- Buttons: `View` (routes to Diagnostics overview, NOT directly into the inspector — user clicks the Crashes card to enter; this avoids consuming the read action implicitly) and `Dismiss`.
- 8 s auto-dismiss, hover-to-pause. Closing the toast does NOT clear the rail dot — only opening a crash does.

**No modal interrupt** under any path. The first user impression of a crashed app must not be a forced dialog.

### Bulk delete

`Delete all` button in the inspector pane header (right side, destructive-ghost). Triggers an AlertDialog: "Delete all crash reports?" with body copy "<N unread> will be removed too. This can't be undone." Confirm-button label is dynamic: `Delete N reports`. On confirm: removes both the JSON files and the sibling `state.json` entries.

### Report on GitHub (rescoped from opt-in upload — 2026-05-07)

The original v1 design (preserved in git history) was a configurable HTTPS POST to an arbitrary endpoint behind an Includes/Excludes disclosure dialog and a per-endpoint suppress checkbox. That design assumed a multi-collector world OpenWhisper doesn't actually live in: this is an open-source project whose canonical bug tracker is the GitHub Issues queue at `jimmi-joensson/OpenWhisper`. Building + maintaining a generic upload endpoint that, in practice, only the maintainer would ever configure isn't worth the surface area.

Replaced with a single button that opens a prefilled GitHub issue:

- **Button placement:** sheet action footer, ghost variant, alongside `Open crash folder`. Label: `Report on GitHub`. Always visible — no env var gating, no settings schema bump, no dialog stack-up.
- **What clicking does:** opens `https://github.com/jimmi-joensson/OpenWhisper/issues/new?title=…&body=…&labels=bug,crash` in the user's default browser via `tauri-plugin-opener`'s `openUrl`.
  - Title: `Crash report — vN.M.0 — <truncated panic message>` (≤72 chars).
  - Body: the same redacted markdown `formatCrashAsMarkdown` produces for the Copy GitHub-ready report flow. Single source of truth — no second formatter.
  - Labels: `bug,crash`.
- **Body length cap:** GitHub's documented URL limit is fuzzy but ~8 KB is the practical browser ceiling. We truncate the body to ~6 KB (preserving the identity block in full and trimming the backtrace tail), append a `_Truncated — paste the full report from `Copy GitHub-ready report` if needed._` marker, and rely on the existing Copy flow as the fallback for users who hit the cap.
- **Auth shape:** the user has to be signed into GitHub in their browser once. That's a one-time, browser-level auth — not a per-crash auth, not a stored token, not infrastructure OW runs.
- **Privacy:** the redaction step is identical to the Copy flow (per-`String`-field redaction at write time inside the panic hook, formatter inherits clean input). No new disclosure surface needed.

The `Copy GitHub-ready report` flow stays as-is for users who want to paste manually.

### Unread count for TASK-62.8 (Diagnostics overview entry card)

The Diagnostics overview pane (TASK-62.8) renders the **Crashes entry card** sourced from the same `state.json`. The card is the single navigation surface from the overview into the inspector — its unread-count pill (`3 unread`, recording-orange background, white text) and "Last: 2 days ago · recognizer.so · SIGSEGV" mono summary line tell the user what's waiting without forcing them to drill in. Implemented as a Tauri command + `useEffect` poll on the overview pane; no live event needed.

## Open questions / decided trade-offs

- **Schema versioning**: chose explicit `schema_version` integer; alternative would be SemVer string. Integer is simpler and we don't expect rich version semantics.
- **Filename = unix-ms**: trivially monotonic, sortable, no clash on rapid restarts. Trade-off: not human-readable in the file picker — but users are expected to view via the inspector, not the raw folder, so OK.
- **No log-tail**: see Non-goals. Risk is that crashes whose proximate cause is upstream of the panic (e.g. a thread exited without us noticing) will be hard to diagnose. Acceptable for v1; revisit if it bites.
- **Markdown ` ``` ` nesting in copy format**: we use `<details>` blocks containing fenced code. GitHub renders this correctly — verified format pattern is widely used.
- **Crash inside the hook**: best-effort serialize, no double-fault protection. Trade-off considered: a `signal-hook` setup for SIGSEGV/SIGABRT would catch native crashes too, but that's a separate scope and a step toward Crashpad-grade complexity. Not v1.

## Out of scope (deferred)

- Webview JS error → crash file
- Native (non-Rust) crash capture (SIGSEGV, EXCEPTION_ACCESS_VIOLATION) — would need Crashpad-style minidump
- Aggregated / hashed crash signatures, dedupe across users
- A logging facility (`tracing` adoption) — separate task if/when we adopt it
- An issue template under `.github/ISSUE_TEMPLATE/` aligning with the copy format — nice complement, can be filed separately
