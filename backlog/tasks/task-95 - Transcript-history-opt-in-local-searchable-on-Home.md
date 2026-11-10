---
id: TASK-95
title: Transcript history - opt-in, local, searchable on Home
status: In Review
assignee: []
created_date: '2026-11-10T14:36:00Z'
updated_date: '2026-11-10T15:28:00Z'
labels:
  - history
  - core
  - cli
  - tauri
dependencies:
  - TASK-94
priority: high
ordinal: 77000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Keep a local history of dictations and show it on Home. Today no transcript text is stored: the dictations table has a transcript column that is always NULL, and Home shows only the latest transcript from memory.

Decisions (2026-11-10):
- Opt-in. Off by default; Home offers to switch it on. Earlier audit (doc-36, C4) set the rule 'transcripts on disk only if asked'.
- Retention is a setting: 7, 30, 90 days or forever. Default 30 days. Older text is removed automatically.
- Each entry stores the final text, the original text before Refine when Refine changed it, and the target app (from the focus context task), on top of the existing time, duration and word count.
- The list lives on Home: grouped by day, with search, copy, delete one and clear all. No new sidebar item.
- Local only. Plain SQLite in the app's config folder, same protection as dictionary.json. No sync, no audio.
- Switching history off stops saving; existing entries stay until cleared or until retention removes them. A pause switch stops saving without changing the setting.

Stats must keep working with history off: the row is still written, only the text columns stay empty. Removing text (retention, delete, clear) keeps the stats row.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 History is off by default; with it off no transcript text is written to disk and stats are unchanged
- [ ] #2 With history on, each dictation stores final text, original text when Refine changed it, and target app id and name
- [ ] #3 Retention setting (7, 30, 90 days, forever; default 30) removes older text automatically and keeps the stats row
- [ ] #4 Home shows the history grouped by day with search, copy, delete one and clear all; empty and switched-off states are clear
- [ ] #5 Pause stops saving without clearing; delete and clear remove the text for good
- [ ] #6 Headless parity: openwhisper history list, search, show, delete and clear use the same core functions as the app
- [ ] #7 Settings say plainly what is stored and where the file lives
- [ ] #8 Tests cover the store functions, retention, search and the Home list (Playwright)
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
See doc-48 and the subtask notes. Changes to the description's decisions, made with the user on 2026-11-10 while testing:
- The text is stored as Markdown files, one per day, in a folder (default history/ next to dictionary.json, movable), not in SQLite. The files are the only copy; the store keeps counters only.
- No separate pause: the one switch stops saving and keeps what is stored.
- Home shows the newest 5; a History screen has the full list with search and pages; clear all is there.
- Reset stats does not touch history.
Windows: compiles, not run. Idea noted while testing: a dictation into a password manager was stored like any other; a list of apps whose dictations are never stored would fit with the never-capture list in TASK-98.
<!-- SECTION:NOTES:END -->
