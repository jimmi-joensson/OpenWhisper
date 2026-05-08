---
id: TASK-62
title: Model memory telemetry + lifecycle foundation
status: Done
assignee: []
created_date: '2026-04-30 22:16'
updated_date: '2026-05-08 07:00'
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
- [x] #1 Cross-platform memory query primitive returns RSS/peak/wired per process and per loaded model
- [x] #2 ModelHandle<T> state machine exists in core (Unloaded → Loading → Loaded → Active) with idle-timer driven release
- [x] #3 Existing Parakeet recognizer (Mac FluidAudio + Win sherpa-onnx) wrapped in ModelHandle without behavior regression
- [x] #4 Tauri commands surface telemetry + state to UI; Diagnostics panel renders live RAM + state per model
- [x] #5 Settings: 'Keep models warm' toggle persists and overrides idle timeout
- [x] #6 Playwright covers diagnostics panel + setting toggle
<!-- AC:END -->

## Implementation Notes
<!-- SECTION:NOTES:BEGIN -->
Shipped in v0.6.0 across two PRs:

- **PR #18 (TASK-62.1–62.10)**: cross-platform memory primitive, `ModelHandle<T>` state machine with idle timer, Parakeet wrapped on both Mac (FluidAudio) and Windows (ort), `telemetry_get_memory` + `model-state-changed` events, Diagnostics → Memory pane with sparkline + system memory readout, Keep-models-warm toggle, Playwright coverage.
- **PR #22 (TASK-62.11–62.13, Stream B)**: single-bar Diagnostics RSS Breakdown, Settings → Models memory budget bar with hover-ghost preview + per-row delta chips, Settings → Models storage panel with platform-aware Show in Finder/Explorer.

Late polish on the v0.6.0 release smoke (commit on the 0.6.0 tag):
- Diagnostics breakdown bar renamed to "OpenWhisper Memory Breakdown"; Parakeet segment now sources from process RSS on Windows and from the ANE/GPU claim on Mac (`Parakeet weights (ANE)` legend label) so the bar total matches the OpenWhisper Memory readout above.
- Bar's resident readout switched to honest MB / GB units (sub-1 GB renders MB).

Shipped publicly with v0.6.0 on 2026-05-08 — Mac DMG + Windows MSI/NSIS exe attached:
https://github.com/jimmi-joensson/OpenWhisper/releases/tag/v0.6.0
<!-- SECTION:NOTES:END -->
