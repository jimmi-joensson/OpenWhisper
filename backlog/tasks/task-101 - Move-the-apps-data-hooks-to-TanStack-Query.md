---
id: TASK-101
title: Move the app's data hooks to TanStack Query
status: To Do
assignee: []
created_date: '2026-11-10T15:07:00Z'
labels:
  - tauri
  - frontend
dependencies: []
priority: low
ordinal: 92000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The webview reads its data from Rust through hand-written hooks: useEffect plus invoke plus an event listener, one per feature (stats, Refine, dictionary, crashes, settings). Each keeps its own copy, so two screens showing the same setting can disagree until one re-mounts, and each re-implements loading, errors and refresh.

Transcript history (TASK-95) was built on TanStack Query instead: one cache, one copy per value, invalidated when Rust says the store changed. The user asked on 2026-11-10 to look at state management in general and prefers TanStack. This task moves the remaining hooks over, one feature at a time, starting with the ones shown in two places (Refine settings: Home switch and Settings pane).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Refine settings and status read through TanStack Query; the Home switch and the Refine pane share one cached value
- [ ] #2 Stats, dictionary and crash hooks moved, each with its Playwright spec still green
- [ ] #3 No hook keeps a private copy of a value another screen also shows
- [ ] #4 The Rust events that mean 'data changed' invalidate the matching queries in one place
<!-- AC:END -->
