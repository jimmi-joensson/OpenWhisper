---
id: doc-43
title: Diagnostics + Models design polish from 2026-05-07 handoff
type: specification
created_date: '2026-05-07 13:56'
---

**Backlog parent:** TASK-62 (Model memory telemetry + lifecycle foundation) — these are net-new subtasks under 62, not patches to existing 62.x work.

**Companion:** TASK-78 (crash reporting) — the *crash inspector* UX changes from the same design handoff are patched in place into `backlog/docs/specs/doc-22 - Crash-reporting-in-app-inspector.md` + `backlog/docs/plans/doc-23 …`. This doc covers only the **non-crash** Diagnostics + Settings → Models polish.

**Design source:** `chats/chat6.md` (Diagnostics Panel Design, 2026-05-04) and `chats/chat7.md` (Crash Inspector UI, 2026-05-04) inside the design bundle the user supplied on 2026-05-07. Reference HTML/JSX prototypes: `project/diagnostics.jsx` (DiagnosticsBoard, SystemMemorySection, BreakdownBar, MemoryBudgetBar, ModelsStoragePanel) and `project/crashes.jsx` (CrashEntryBoard / CrashOverviewMock for the entry-card pattern reused on the Diagnostics overview).

## Problem

The Diagnostics pane (TASK-62.8, In Review) currently shows a single Memory section with a dual-line system + OpenWhisper RSS chart, pressure legend, and a stats grid. Two gaps surfaced in the design pass:

1. **The memory readout doesn't say what's *inside* OpenWhisper's RSS.** The user can see "we're using 1.10 GB" but not "612 MB of that is the Parakeet model, 286 MB is the app shell, 142 MB is audio buffers, 84 MB is caches." That breakdown is the answer to "is OpenWhisper itself heavy?" — the question the pane exists to answer.

2. **The Settings → Models pane has no memory cost surface.** Today, toggling a model on simply loads it; the user finds out about the memory hit by checking Diagnostics afterward — which is the *debugging* surface, not the *decision* surface. The design pass landed on a horizontal **memory budget bar** at the top of the Models pane: anchored to physical RAM, with segments for system + other apps, OpenWhisper base, and each enabled model — plus a hover-revealed *ghost segment* showing what enabling a not-yet-loaded model would cost. This is the load-bearing addition.

A third, smaller gap: the Settings → Models pane today says nothing about disk. Users who download a 4.92 GB Llama model have no way to find or audit it inside the app. We add a small **Storage panel** below the model list with the disk total, install count, the canonical models folder path, and a "Show in Finder" / "Show in Explorer" button.

## Goals

- **OW RSS Breakdown bar** in the Diagnostics → Memory section. Stacked horizontal bar showing the four canonical components of OpenWhisper's resident memory with per-segment labels and percentages.
- **Memory budget bar** in Settings → Models. Stacked horizontal bar anchored to physical RAM, with per-model segments, hover-ghost preview of what enabling a disabled model would cost, and matching `+<MB>` delta chips on the model rows.
- **Storage panel** in Settings → Models. Inline strip showing total disk used by enabled models, count, mono path, and a "Show in Finder/Explorer" button via `tauri-plugin-opener`.
- **Crashes entry card** on the Diagnostics overview pane. Section card pattern matching the existing Memory section, with an unread-count pill and last-crash mono summary line. (This is also referenced from doc-22 / TASK-78.3 — listed here for completeness; the implementation lives in TASK-78.3.)

## Non-goals

- A Performance section on the Diagnostics pane. The design includes a `PREVIEW`-tagged Performance section (RAF p50/p99, audio-callback latency, recognizer-step ms), but real telemetry for those counters doesn't exist yet — adding a stub block with hand-mocked numbers would lie to the user. Defer until TASK-NN (stutter-diagnosis spike) lands the actual counters; at that point Performance gets its own task.
- Donut / vertical-bar variants of the Diagnostics breakdown. The user explicitly chose to keep the Diagnostics breakdown and the Models budget as two separate horizontal bars (different scopes — process-internal vs. system-wide). See chat6.md final exchange.
- Eviction-cost preview / "if I enable Llama and Parakeet evicts" interactions on the budget bar. Hover-ghost is additive only ("what does enabling cost"); the inverse ("what would removing free") is captured by the bar's existing legend update on toggle, not by a separate hover surface.

## Design decisions

### Two horizontal bars, on purpose

|  | Diagnostics breakdown | Models budget |
|---|---|---|
| **Scope** | What's inside OpenWhisper's RSS (~1.1 GB) | What's inside the whole 24 GB box |
| **Frame** | Process-internal forensics | System-wide budget planning |
| **Categories** | Parakeet weights / audio buffers / shell / caches | System & other apps / OW base / per-model claims / Headroom |
| **Reader's question** | "Why does OpenWhisper use 1.14 GB?" | "Can I afford to enable another model?" |
| **State** | Reactive (live measurement) | Predictive (with hover-to-preview) |

The bars look similar but answer different questions. We keep both. Section titles distinguish them: "OpenWhisper RSS Breakdown" (Diagnostics) vs. "Memory budget" (Models).

### OW RSS Breakdown — segment composition

Four segments, ordered by size:

- **Parakeet weights** — the loaded recognizer model's resident memory. Color: `var(--info)`. Source: per-model RAM attribution from `useMemoryStats` (TASK-62.7's telemetry feed).
- **Audio buffers** — circular buffers for the audio tap, the resampler, and the active recognizer's input window. Color: `color-mix(in oklch, var(--info) 65%, var(--background))`.
- **App shell** — Tauri webview + Rust shell process baseline. Color: `color-mix(in oklch, var(--info) 35%, var(--background))`.
- **Caches** — model file mmap residency, settings, transcription history cache. Color: `color-mix(in oklch, var(--muted-foreground) 60%, transparent)`.

Total reads from the live RSS ticker; segment values are **estimates** (RSS-delta attribution, not exact). The pane already carries that caveat in its footer; the breakdown bar inherits it. Per-segment labels show `<%> · <MB>` in mono.

### Memory budget bar — segment composition

Anchored to **physical RAM** (looked up once at boot via `sysinfo` crate, then static — physical RAM doesn't change at runtime on either platform we ship). Segments left-to-right:

- **System & other apps** — `physicalMb - openWhisperRssMb - headroomMb`. Faded (opacity 0.65), neutral white.
- **OpenWhisper base** — fixed-ish baseline (~380 MB at boot before any model loads). Mid-white.
- **Per-enabled-model claims** — one segment per `enabled === true` model in the catalog, each in the model's accent color (Parakeet → recording-orange, Llama → amber, Qwen → violet, Whisper → cyan).
- **Headroom** — transparent, fills remainder. Color of the headroom *number* in the header swaps amber on hover-ghost (i.e. when previewing an additive model load) and green on hover-remove.

**Hover-ghost preview** (the load-bearing UX): hovering a not-yet-enabled model row in the list below the bar reveals a striped segment in the bar (same color as that model's accent), animated with the existing `diag-pulse` keyframe, dashed-bordered, AND a `+<MB>` delta chip on the model's toggle. The Headroom number in the bar header swaps to `<new> (was <old>)`. Hovering an enabled model row marks its existing segment as departing (line-through legend label, `−<MB>`, dashed outline on the bar segment) and shows the headroom recovery in green.

### Storage panel — composition

Inline strip below the models list. Single line on rest:

```
<total-GB> on disk · <N> models installed · <path>          [Show in Finder]
```

Path is mono, ellipsized with `text-overflow: ellipsis` on overflow. The button is the same `Open crash folder` ghost button the crash inspector uses (TASK-78.4) — `tauri-plugin-opener` invocation. Path resolves at runtime from `app.path().app_data_dir()` + `models/` (matches the existing Parakeet/FluidAudio bundle convention; never hard-coded).

Disk math: sum of `model.diskMb` for enabled models. Until each model record carries a real `diskMb`, fall back to `ramMb * 0.78` as a placeholder (matches the heuristic the design prototype used). A real `diskMb` lands on the model record in TASK-62.NN whenever the catalog gains a "where this came from" field; the storage panel reads whichever is available.

### Crashes entry card — composition

Implemented under TASK-78.3, repeated here for spec completeness:

- 28×28 destructive-tint tile with crash glyph
- "Crash reports" label + recording-orange `<N> unread` pill (white text, font-mono 10px, padding 1×6)
- Mono "Last: <relative> · <module> · <signal>" sub-line (e.g. "Last: 2 days ago · recognizer.so · SIGSEGV")
- Right-side chevron
- Whole card is the click target (`Button`-styled, full-width, sunken background, rounded 8). Tap → swap Diagnostics pane to crash list (TASK-78.3).
- When `unread === 0 && total === 0`, the card is replaced by a one-line muted "No crashes recorded · Open crash folder" — silence is louder than a missing affordance.

## UI architecture

All three additions are **net-new components** under `apps/tauri/src/components/`:

- `diagnostics-rss-breakdown.tsx` (new) — read-only stacked bar with the four canonical segments.
- `models-memory-budget-bar.tsx` (new) — interactive stacked bar with hover-ghost preview. Composes against the existing `useMemoryStats` hook (TASK-62.x) for live RSS and physical RAM.
- `models-storage-panel.tsx` (new) — inline strip with path + opener.

The Diagnostics overview pane gains a new `<CrashesEntryCard>` rendered inside the existing `Crashes` `<section>` block — that card is owned by TASK-78.3.

The Settings → Models pane (currently `apps/tauri/src/components/settings-models-pane.tsx` if it exists, or wherever the model list lives — verify at impl time) gains the budget bar above the list and the storage panel below it. Model rows gain hover state that drives the budget bar's ghost segment via a single shared `hoveredModelId` piece of pane-local state.

## Out of scope (deferred)

- Performance section on Diagnostics (deferred until real counters land)
- Per-segment time-series for the RSS breakdown (today's bar is a single live snapshot; sparking each segment over time is interesting but not load-bearing)
- Donut / pie variants of either bar
- Cost preview when *removing* an enabled model with hover-ghost (today the bar already handles toggle animation; an explicit "if I disable this, here's what frees" hover is redundant)
- A unified resources panel that bundles memory + disk into one card on Settings → Models (the user asked specifically to keep them separate; see chat6 final exchange)
