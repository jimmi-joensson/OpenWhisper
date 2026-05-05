---
id: TASK-82.4
title: >-
  Plan Task 4: Cache strategy — cargo registry, target/, ort, Playwright, pnpm
  store
status: To Do
assignee: []
created_date: '2026-05-04 15:47'
updated_date: '2026-05-04 15:51'
labels:
  - 82-impl
milestone: m-1
dependencies: []
parent_task_id: TASK-82
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add caching layers to all three jobs to bring warm-cache runtime to ~5 min/job. Cache cargo registry/git/target keyed on Cargo.lock + runner OS; ort runtime keyed on OPENWHISPER_ORT_VERSION; Playwright browsers keyed on pnpm-lock.yaml. pnpm store comes via setup-node@v4 cache:'pnpm'.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Warm-cache PR run finishes in ~15 min wall (sum of three jobs in parallel)
- [ ] #2 Cold-cache run finishes in ~40 min wall
- [ ] #3 Bumping OPENWHISPER_ORT_VERSION in fetch-ort.cjs auto-invalidates the ort cache (version is in cache key)
- [ ] #4 Bumping a dep in Cargo.toml auto-invalidates cargo registry + target cache
- [ ] #5 Each cache step shows 'Cache restored from key' (not 'Cache not found') on the second PR run
- [ ] #6 cargo registry + target/ cache uses Swatinem/rust-cache@v2 (not hand-rolled keys) — handles target eviction correctly
<!-- AC:END -->
