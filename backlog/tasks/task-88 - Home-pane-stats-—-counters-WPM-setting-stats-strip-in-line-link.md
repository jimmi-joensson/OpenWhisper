---
id: TASK-88
title: 'Home-pane stats — counters, WPM setting, stats strip, in-line link'
status: To Do
assignee: []
created_date: '2026-05-06 06:10'
updated_date: '2026-05-06 06:16'
labels: []
dependencies:
  - TASK-87
documentation:
  - backlog/docs/specs/doc-41 - Home-pane-stats-—-design.md
  - backlog/docs/plans/doc-42 - Home-pane-stats-—-implementation-plan.md
ordinal: 52000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add a 4-card stats strip on the Home pane (Words Today, Words This Week, Words All-Time, Time Saved) backed by writes to the dictations table from TASK-87. New Stats settings pane with user-WPM input (default 40, clamp 10–300) plus Reset Stats button. Time-saved calc is words/wpm − seconds/60, clamped at 0. The 'X wpm' inside 'vs. typing at X wpm' is a shadcn Button variant=link with a Settings icon suffix that routes to the Stats settings pane. Empty state on first launch (zeros + dashes). Increments only on injection-success path, never on cancel/empty/failed dictations.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Each successful dictation INSERTs one row into dictations (started_at, duration_ms, word_count) from dictation_deliver_transcript after injection — no insert on cancel/empty/failed paths
- [ ] #2 Tauri cmd stats_get_summary returns { words_today, words_week, words_all_time, seconds_total } via SUM aggregations; Tauri cmd stats_reset DELETEs all rows
- [ ] #3 Settings store gains user_wpm field (default 40, integer, clamp 10–300 with helper text on out-of-range); persists in the existing JSON store, NOT in SQLite
- [ ] #4 New Stats settings pane registered in SETTINGS_PANES between Models and Shortcuts; pane id = stats; renders WPM input + Reset Stats button
- [ ] #5 Home pane shows 4-card StatsStrip above the hero; empty state = 0 / 0 / 0 / — with subcaptions matching the mockup (across this Mac, last 7 days, since first launch, vs. typing)
- [ ] #6 Time-saved card subcaption renders 'vs. typing at <wpm> wpm' where <wpm> is a shadcn Button variant=link with lucide Settings icon suffix that routes to the Stats settings pane
- [ ] #7 Playwright spec covers: empty state renders zeros + dashes, increment after a simulated dictation, Reset Stats wipes counters back to empty state, link click navigates to Stats pane
<!-- AC:END -->
