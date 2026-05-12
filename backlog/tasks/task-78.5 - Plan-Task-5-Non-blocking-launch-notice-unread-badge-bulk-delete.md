---
id: TASK-78.5
title: 'Plan Task 5: Delta-driven launch toast + persistent rail dot + bulk delete'
status: In Review
assignee:
  - '@claude'
created_date: '2026-05-04 06:16'
updated_date: '2026-05-12'
labels:
  - 78-impl
milestone: m-1
dependencies: []
parent_task_id: TASK-78
ordinal: 38000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Settings schema gains last_seen_unread_count: u32 (default 0), persisted across restart
- [x] #2 Launch toast fires only when currentUnread > lastSeenUnread; subsequent restarts at same/lower unread show only the rail dot
- [x] #3 Rail dot persists until each unread crash is explicitly marked read; never auto-dismissed by route visits, time-out, or toast dismiss
- [x] #4 Toast 'View' button routes to Diagnostics overview (not the inspector) — entering the inspector requires the user's explicit click on the Crashes entry card, which is the read action
- [x] #5 Delete-all empties both crash files and state.json entries; confirm dialog uses dynamic count copy
- [~] #6 RETIRED — crashes_summary command never built; entry-card sub-line is derived in JS from the existing crashes_list payload (see Playwright "entry card surfaces unread pill + last-crash sub-line"). Dedicated command was redundant.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
76206ff shared crashes-store via useSyncExternalStore; sidebar rail dot in lockstep with list. Implements rail-dot AC #3.

**Post-v0.6.0 audit (2026-05-12):**
- AC #3 (rail dot) shipped in v0.6.0 via 76206ff.
- AC #5 (Delete-all + dynamic confirm copy) shipped in v0.6.0 — `crashes_delete_all` truncates both crash files and `state.json` via `save_state(&dir, &CrashesState::default())` at `apps/tauri/src-tauri/src/crashes/mod.rs:223-258`. AlertDialog at `apps/tauri/src/components/crash-list.tsx:130-148` uses dynamic count copy (`{unreadCount} unread will be removed too` + `Delete {total} reports`). Playwright covers it at `apps/tauri/tests/crash-inspector.spec.ts:242`.
- AC #6 retired: the entry-card sub-line is derived in JS from the existing `crashes_list` payload rather than a dedicated `crashes_summary` command. Playwright proves the sub-line works (`crash-inspector.spec.ts` — "entry card surfaces unread pill + last-crash sub-line"). The dedicated command was redundant.

Remaining: ACs #1, #2, #4 — the delta-driven launch toast (settings `last_seen_unread_count` field + boot-time comparison + toast component + View-button routes to Diagnostics overview). One discrete chunk; task stays In Progress until that ships.

**Update — delta-driven launch toast ships (2026-05-12):**

Stored next to the existing per-crash UI flags rather than in the user-settings JSON — `last_seen_unread_count: u32` lives on `CrashesState` in `apps/tauri/src-tauri/src/crashes/mod.rs`, since it's a state value the app maintains rather than a user preference. Persisted into the existing `state.json` (atomic-on-rename pattern shared with the per-crash entries).

Two new Tauri commands: `crashes_get_last_seen_unread() -> u32` and `crashes_mark_seen(count: u32) -> Result<(), String>` (idempotent — writing the same value twice is a no-op).

React side: the shared `crashes-store` polls + tracks `lastSeenUnreadCount` alongside `unreadCount`. New `markSeen()` mutator is called by:

- The Diagnostics → Crashes **entry card** click (`diagnostics-pane.tsx`) — the explicit per-AC #4 "read" action.
- The launch toast's **View** button — also navigates to Diagnostics overview (NOT the inspector).
- The launch toast's **Dismiss** button — acknowledges without navigating.

New component `apps/tauri/src/components/crashes-launch-toast.tsx` (uses the existing `Alert` + `Button` primitives — no new dep). Mounts in `App.tsx` above the route body, latches its "should show" decision on first non-loading store snapshot so a mid-session new crash doesn't re-pop the boot notice, and unmounts once dismissed.

Playwright (5 new tests under `crash-inspector.spec.ts`):

- toast fires when `currentUnread > lastSeenUnread`
- toast stays hidden when `currentUnread <= lastSeenUnread` (rail dot still visible)
- View button routes to Diagnostics overview + bumps `last_seen`
- Dismiss button bumps `last_seen` without navigating
- entering inspector via entry card bumps `last_seen` even with the sentinel-suppressed default

Shim default for `crashes_get_last_seen_unread` is `Number.MAX_SAFE_INTEGER` so existing crash-inspector tests don't see the toast by default; the new spec block sets `__owCrashesLastSeenUnread = 0` (or any value lower than the seeded unread count) to exercise the boot delta.

Verification: pnpm test:ui 122/122 passes (was 117 before — 5 new tests added).

Now In Review.
<!-- SECTION:NOTES:END -->
