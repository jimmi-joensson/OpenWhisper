---
id: TASK-62.8
title: 'Plan Task 8: Diagnostics panel UI'
status: In Review
assignee: []
created_date: '2026-04-30 22:26'
updated_date: '2026-05-07 00:00'
labels:
  - 62-impl
dependencies: []
parent_task_id: TASK-62
ordinal: 11000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Diagnostics sidebar entry visible in Settings window
- [x] #2 Pane renders process RSS + per-model rows, refreshes every ~1 s
- [x] #3 State updates propagate immediately on model-state-changed
- [x] #4 Estimate caveat surfaced in pane footer
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
- AC interpretation: Diagnostics is no longer a Settings sub-pane — it landed earlier as a top-level Route alongside Home and Settings (App.tsx, sidebar-nav.tsx). AC#1 closed against that sidebar entry. AC#2 "per-model rows" reinterpreted as the breakdown-bar segments (one per loaded ModelHandle); per the design (`chats/chat9.md`), the row-style decision surface lives in Settings → Models, deferred to a follow-up task.
- New `apps/tauri/src/lib/use-memory-stats.ts`. 1-Hz `setInterval` poll of `telemetry_get_memory` + `listen("model-state-changed")` for instant refetch between polls. 60-sample ring of `process.rss_bytes` (matches design's "Last 60 s" caption). Hook intentionally lives in `lib/` — future Settings → Models pane will consume the same telemetry without spinning up a second poll.
- New `apps/tauri/src/components/diagnostics-pane.tsx`. Memory card: two readouts (OpenWhisper RSS + Models loaded), 60-sample area sparkline (transform/opacity-only — T3 surface, no idle motion, reduced-motion safe), breakdown bar with one info-tinted segment per `models[]` row that has `estimated_rss_bytes > 0` plus a muted "Other" remainder. Footer note explains the RSS-delta estimate caveat (per-load snapshot, ANE-resident memory invisible on macOS) and points to OS Activity Monitor for system-wide pressure.
- "Other" remainder uses saturating subtraction (`max(0, rss - sum_models)`) so live RSS dipping below the sum (compaction, tail-merge) renders as zero, not a negative stripe.
- App.tsx: dropped the entire DiagnosticsPaneProps surface (phase / levels / transcript / RecordButton onToggle / coreVersion); the new pane is self-driven. Removed the now-unused `core_version` invoke + state — `general-pane.tsx` already fetches it for the Updates row.
- `App.css` adds `.ow-diagnostics*` styles (~190 lines). Tabular-nums on all numeric readouts. Segment colors via semantic tokens (`var(--info)`, `color-mix` with `var(--muted-foreground)`) — no raw hex.
- main-window.spec.ts: ported `getByText("Rust ↔ React FFI")` and `getByText("transcript", { exact: true })` assertions to anchor on the new pane (Diagnostics heading + Memory card visible-in-viewport + footer caveat for the scroll test).
- `pnpm test:ui` 86/86 green. tsc clean.
- Awaiting user QA — `pnpm dev:tauri` smoke: open Diagnostics with `recognizer` registered, dictate, observe RSS bump in sparkline + breakdown segment grow on Loaded.
- Commit: 09714e4.
- Polish (commit ab82ceb): replaced the per-tick path-redraw with classic Activity-Monitor left-scroll. Samples pinned to fixed viewBox-x positions (latest at right edge); the containing `<g>` translate3ds left by one sample-width over the tick interval, snapping back at swap time. Path-data index shift preserves screen positions across the swap (no jump). Animation state in refs only, transform-only, reduced-motion short-circuits to discrete snap. ResizeObserver converts the per-sample step into CSS-pixel distance so the slide aligns with the visual sample pitch on any container width. Trailing-edge dot dropped — would have popped one sample-width at every swap.
<!-- SECTION:NOTES:END -->
