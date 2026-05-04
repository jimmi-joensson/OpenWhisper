---
id: TASK-78
title: Crash reporting — Rust panic hook + on-disk dump + opt-in upload
status: To Do
assignee: []
created_date: '2026-05-04 05:43'
updated_date: '2026-05-04 08:03'
labels: []
dependencies: []
documentation:
  - backlog/docs/specs/doc-22 - Crash-reporting-in-app-inspector.md
  - backlog/docs/plans/doc-23 - Crash-reporting-in-app-inspector-—-plan.md
priority: high
ordinal: 33000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
First-class crash capture across macOS and Windows. Currently a Rust panic in the core (e.g. observed once on Windows during a long recording) is lost — no backtrace, no repro signal. This task adds a panic hook, on-disk crash file with backtrace + recent log tail, and — most importantly for an OSS app whose users file GitHub Issues — an **in-app crash inspector** so users can see past crashes, read the report, and copy a GitHub-ready summary to paste straight into an issue. Optional opt-in upload is layered on top but is not the primary surface. File-based first; no Sentry/Crashpad SDK yet — revisit once we have real crash volume.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 panic::set_hook installed in core; panics serialize backtrace + last N log lines + app version + OS + recording-state-at-crash to ~/.openwhisper/crashes/<ts>.json
- [ ] #2 On next launch, app detects unread crash files and surfaces a non-blocking notice (toast or settings-pane badge) — no modal interrupt
- [ ] #3 Crash inspector UI in Diagnostics (or dedicated settings sub-pane): list of crash reports newest-first showing timestamp, app version, OS, short cause line
- [ ] #4 Detail view per crash shows full backtrace + log tail in a scrollable, monospaced panel
- [ ] #5 "Copy GitHub-ready report" button on detail view: copies a markdown block with collapsible <details> backtrace, app version, OS, and a placeholder "What I was doing" section ready to paste into a GitHub Issue
- [ ] #6 "Open crash folder" button as fallback for users who want the raw file
- [ ] #7 Per-crash actions: mark as read, delete; bulk "delete all" with confirm
- [ ] #8 Optional opt-in upload to a configurable endpoint (default no-op stub so Mac+Win builds work offline) — surfaced as a per-crash button, NOT a one-time launch modal
- [ ] #9 Unread crash count exposed to Diagnostics panel (TASK-62.8) once available
- [ ] #10 Manual repro: trigger panic via debug menu on both macOS and Windows; file lands in expected dir; next launch shows it in the inspector list; copy button produces a paste-ready markdown report
- [ ] #11 No PII in crash file by default (no transcript text, no audio, no file paths under /Users/<name> beyond redaction); copy-to-clipboard uses the same redaction
- [ ] #12 Inspector lives under the existing top-level **Diagnostics** route (not Settings) — Settings panes stay config-only; Diagnostics is read-only data + actions
- [ ] #13 If TASK-62.8 has already shipped a single-pane Diagnostics view by the time this task lands, this task introduces the Diagnostics sub-sidebar with two entries: "Overview" (the existing model RAM/state pane from 62.8) and "Crashes" (the new inspector). Do NOT add the sub-sidebar speculatively before there are ≥2 sections to navigate between
<!-- AC:END -->
