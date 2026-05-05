---
id: TASK-85.3
title: 'Plan Task 3: Cargo workspace rename'
status: To Do
assignee: []
created_date: '2026-05-04 16:35'
updated_date: '2026-05-04 16:40'
labels:
  - 85-impl
milestone: m-1
dependencies: []
parent_task_id: TASK-85
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Rename Cargo package names (NOT directory paths — preserves git history). openwhisper-core → <new-cargo>-core; openwhisper-tauri → <new-cargo>-tauri; openwhisper-cli → <new-cargo>-cli. Update every use openwhisper_core::* import (~30-50 sites). Regenerate Cargo.lock. Verify cargo check --workspace clean on Mac + Win.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 All workspace crate name fields use <new-cargo>-* (verified via cargo metadata)
- [ ] #2 Every use openwhisper_core::* import replaced with use <new_cargo>_core::*
- [ ] #3 Cargo.lock regenerated and committed
- [ ] #4 [profile.dev.package.<new-cargo>-core] override in root Cargo.toml updated
- [ ] #5 Directory paths under core/, apps/tauri/src-tauri/, cli/ unchanged — git log shows continuous history
- [ ] #6 Workspace cargo check passes on Mac and Windows under TASK-82's flag set (--exclude bench-sherpa --no-default-features --features tauri)
<!-- AC:END -->
