---
id: doc-22
title: Crash reporting + in-app inspector
type: spec
created_date: '2026-05-04 06:12'
---

**Backlog parent:** TASK-78

## Problem

A Rust panic in OpenWhisper's core or Tauri shell is currently lost: no backtrace, no record on disk, no way for the user to forward it. We saw this concretely once on Windows — a long recording crashed and the only signal was the user's memory of the event. With ~zero crash signal we can't diagnose anything that isn't reproduced live in front of us.

OpenWhisper is open-source and most reports come in as GitHub Issues. The most valuable thing we can give a user is a crash report they can paste into an issue verbatim, with no shell hunting and no manual editing.

## Goals (v1)

- Capture every Rust panic that reaches the panic hook with backtrace + minimal context, written to a versioned JSON file in the OS-correct app log directory.
- Show captured crashes inside the app under a Diagnostics route. List + detail view, no modal interrupts.
- One-click "Copy GitHub-ready report" producing redacted markdown ready to paste into an Issue.
- Optional opt-in upload (per-crash, not blanket-consent) to a configurable endpoint. Default endpoint is a no-op stub — builds work fully offline.
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

Sub-sidebar inside Diagnostics is conditional:

- If TASK-62.8 (model RAM/state pane) has already shipped a single-pane Diagnostics view by the time TASK-78 lands → introduce the Diagnostics sub-sidebar with two entries: "Overview" (the existing 62.8 pane) and "Crashes" (the new inspector).
- If TASK-62.8 has NOT yet shipped → render the crash inspector as the single Diagnostics pane. TASK-62.8 will introduce the sub-sidebar when it adds Overview alongside.

Either way: do not pre-build empty sub-nav.

### List view

- Newest-first list of crash files
- Each row: timestamp (relative + absolute), app version, OS, one-line cause (`rust_panic.message` truncated to ~80 chars), unread indicator
- Empty-state: "No crashes recorded. 🦄" (no emoji — placeholder text, copy decided in task)
- Per-row actions in a `…` menu: Mark as read, Delete

### Detail view

- Header: full timestamp, app version, OS, model kind, recording phase if any
- Backtrace pane: monospaced, scrollable, syntax-soft (no language highlighter, just whitespace-pre + monospace)
- Events pane: collapsible, table of recent events from the ring buffer
- Action row:
  - **Copy GitHub-ready report** (primary)
  - **Open crash folder** (secondary)
  - **Upload to support endpoint** (tertiary, only visible if upload endpoint is configured)
  - **Delete** (destructive, confirm dialog)

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

On app start, count unread files in the crashes dir. If ≥1:

- Show a small toast (or sidebar badge — pick during impl) saying "OpenWhisper recorded N crash report(s). View in Diagnostics."
- The toast is dismissable; a sidebar badge persists on the Diagnostics nav item until the user opens the inspector.
- **No modal interrupt.** The very first user impression of a crashed app must not be a forced dialog.

### Bulk delete

"Delete all" button at top of list, confirm dialog, removes both the JSON files and the sibling `state.json` flags for those entries.

### Opt-in upload

- Per-crash button, only enabled if `OPENWHISPER_CRASH_UPLOAD_URL` env var or build-time const is set to a non-empty value.
- Default endpoint: empty string — the button is disabled with a tooltip "No upload endpoint configured for this build."
- Upload is a single HTTPS POST of the JSON file body to the endpoint. No retry, no queue. If it fails, surface the error in the detail view; the file stays on disk for next attempt.
- Sets `state.uploaded_at` on success — UI shows ✓ but file is not deleted.
- Privacy copy: explicit confirm dialog before first upload showing exactly what fields are sent ("backtrace, OS, app version, recording state — no transcripts, no audio, no file paths").

### Unread count for TASK-62.8

The Diagnostics Overview pane (TASK-62.8) renders an unread-crashes counter sourced from the same `state.json`. Implemented as a Tauri command + `useEffect` poll; no live event needed.

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
