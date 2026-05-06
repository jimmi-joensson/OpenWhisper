---
id: TASK-88.4
title: >-
  Plan Task 4: Stats settings pane — register, render WPM input + Reset Stats
  button
status: In Progress
assignee:
  - '@claude'
created_date: '2026-05-06 06:15'
updated_date: '2026-05-06 10:25'
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
- [ ] #2 Editing WPM persists through reload; Reset confirm calls stats_reset; cancel does nothing
- [ ] #3 Home stats strip re-renders to empty state within one event-loop tick after a successful reset (asserted in T7 Playwright)
- [ ] #4 StatsPane renders Typing-speed section (FieldGroup + Field + numeric Input + FieldDescription containing the verbatim spec helper-text quote, NOT a paraphrase) and a Danger-zone Card section (border-destructive/40 bg-destructive/5, CardTitle 'Danger zone' in text-destructive, CardDescription 'These actions are irreversible.', destructive Button + AlertDialog confirm)
<!-- AC:END -->
