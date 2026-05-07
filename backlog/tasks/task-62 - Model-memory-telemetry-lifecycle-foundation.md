---
id: TASK-62
title: Model memory telemetry + lifecycle foundation
status: To Do
assignee: []
created_date: '2026-04-30 22:16'
updated_date: '2026-05-07 14:03'
labels: []
dependencies: []
documentation:
  - backlog/docs/specs/2026-05-01-model-lifecycle-telemetry.md
  - backlog/docs/plans/2026-05-01-model-lifecycle-telemetry.md
  - >-
    backlog/docs/specs/doc-43 -
    Diagnostics-Models-design-polish-from-2026-05-07-handoff.md
  - backlog/docs/plans/doc-44 - Diagnostics-Models-design-polish-—-plan.md
priority: high
ordinal: 1000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Build observability for model memory and an explicit load/unload lifecycle. Validate by wrapping existing Parakeet recognizer. Foundation for the LLM disfluency cleanup feature — independently shippable.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Cross-platform memory query primitive returns RSS/peak/wired per process and per loaded model
- [ ] #2 ModelHandle<T> state machine exists in core (Unloaded → Loading → Loaded → Active) with idle-timer driven release
- [ ] #3 Existing Parakeet recognizer (Mac FluidAudio + Win sherpa-onnx) wrapped in ModelHandle without behavior regression
- [ ] #4 Tauri commands surface telemetry + state to UI; Diagnostics panel renders live RAM + state per model
- [ ] #5 Settings: 'Keep models warm' toggle persists and overrides idle timeout
- [ ] #6 Playwright covers diagnostics panel + setting toggle
<!-- AC:END -->
