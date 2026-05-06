---
id: TASK-81.1
title: 'Plan Task 1: Audit core/ public API + Tauri orchestration leaks'
status: In Review
assignee: []
created_date: '2026-05-04 15:09'
updated_date: '2026-05-06'
labels:
  - 81-impl
dependencies: []
parent_task_id: TASK-81
milestone: m-1
priority: high
documentation:
  - backlog/docs/audits/doc-37 - core-public-api-audit.md
  - backlog/docs/audits/doc-38 - tauri-orchestration-leaks-audit.md
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Read every pub in core/src/ and every #[tauri::command] in apps/tauri/src-tauri/src/. Produce two audit docs as the basis for Tasks 2 and 3.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Audit doc 1 (core public API) committed; lists every pub fn, struct, enum, trait grouped by capability (capture / dictation / transcribe / device-enum / settings / diagnostics)
- [x] #2 Audit doc 2 (shell orchestration leaks) committed; every Tauri shell symbol classified P/O/M with line-number citations
- [x] #3 Concrete extraction checklist named in audit doc 2 ready to feed Task 2
- [ ] #4 Reviewer confirmed coverage: every pub in core/src/ in audit 1; every >5-line function in apps/tauri/src-tauri/src/lib.rs in audit 2
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Two audit docs landed:

- `backlog/docs/audits/doc-37 - core-public-api-audit.md` — every pub in core/src/ across 16 files grouped by capability (capture / device-enum / dictation / transcribe / recognizer / stats / store / verbose / ffi-c). Flags the three missing modules (settings, diagnostics, media_gate), the prelude shape for Task 3, and the `#[non_exhaustive]` checklist.
- `backlog/docs/audits/doc-38 - tauri-orchestration-leaks-audit.md` — every shell symbol in lib.rs (1295 LOC, grew from 1081 since the spec was written), behavior.rs, settings/mod.rs, focus.rs, media_control/mod.rs, permissions/version_reset.rs classified P/O/M with line-number cites. Platform-glue files (hotkey/, fullscreen/, injection/, media_control/{mac,windows}.rs, permissions/{mac,mod}.rs, tray/) confirmed at the file level. Ends with an extraction checklist keyed to commits A→E for Task 2.

AC #4 (reviewer coverage check) waits for human review.
<!-- SECTION:NOTES:END -->
