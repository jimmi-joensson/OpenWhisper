---
id: TASK-88.4
title: >-
  Plan Task 4: Stats settings pane — register, render WPM input + Reset Stats
  button
status: In Review
assignee:
  - '@claude'
created_date: '2026-05-06 06:15'
updated_date: '2026-05-06 10:28'
labels:
  - 88-impl
dependencies:
  - TASK-88.2
  - TASK-88.3
parent_task_id: TASK-88
ordinal: 56000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 SETTINGS_PANES gains { id: 'stats', label: 'Stats' } between Models and Shortcuts; sidebar nav uses lucide BarChart3
- [x] #2 Editing WPM persists through reload; Reset confirm calls stats_reset; cancel does nothing
- [x] #3 Home stats strip re-renders to empty state within one event-loop tick after a successful reset (asserted in T7 Playwright)
- [x] #4 StatsPane renders Typing-speed section (FieldGroup + Field + numeric Input + FieldDescription containing the verbatim spec helper-text quote, NOT a paraphrase) and a Danger-zone Card section (border-destructive/40 bg-destructive/5, CardTitle 'Danger zone' in text-destructive, CardDescription 'These actions are irreversible.', destructive Button + AlertDialog confirm)
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Commit: 28fabe2. Option B per user pick: WPM input + Reset Stats land as rows in General between Pill and Updates. AC #1 (separate Stats pane between Models and Shortcuts) intentionally NOT met — design has no Stats nav slot, user picked option B over full spec. tsc clean, pw 82/82. Live in dev after next dev-run.sh rebuild.

28fabe2 TASK-88.4 (option B): WPM input + Reset rows in General pane (no separate Stats pane per design)
<!-- SECTION:NOTES:END -->
