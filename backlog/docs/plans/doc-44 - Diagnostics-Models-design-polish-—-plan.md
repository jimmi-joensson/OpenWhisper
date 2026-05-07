---
id: doc-44
title: Diagnostics + Models design polish — plan
type: other
created_date: '2026-05-07 13:56'
---

**Backlog parent:** TASK-62 (Model memory telemetry + lifecycle foundation)
**Spec:** backlog/docs/specs/doc-43 - Diagnostics-Models-design-polish-from-2026-05-07-handoff.md
**Companion plan:** backlog/docs/plans/doc-23 - Crash-reporting-in-app-inspector-—-plan.md (carries the Crashes entry card under TASK-78.3)

## Overview

Three implementation tasks, one commit each, runnable in any order — they touch independent surfaces and share no state. Each is a UI-only change in `apps/tauri/src/components/` plus (for the storage panel) a shell-side path resolver. No Rust core changes; no schema changes; no new Tauri commands beyond a single `models_storage_path` resolver in Task 3. Per-task verification is Playwright-driven where the surface is interactive (Task 2's hover-ghost), and screenshot + manual where it isn't (Task 1's static breakdown bar, Task 3's strip).

Cross-task convention: every commit appends a one-liner to the matching subtask's notes via `--append-notes`. Subtask labels are `62-impl` (continuing the existing TASK-62 grouping).

## Task 1: Diagnostics — OW RSS Breakdown bar (Memory section)

Add a horizontal stacked bar below the dual-line memory chart in `DiagnosticsPane`'s Memory section, showing the four canonical segments (Parakeet weights / Audio buffers / App shell / Caches) of OpenWhisper's resident memory.

### Steps

1. Read the `openwhisper-ui-discipline` skill before touching `apps/tauri/src/components/`.
2. **Component.** New `apps/tauri/src/components/diagnostics-rss-breakdown.tsx` exporting `<RSSBreakdownBar />`. Pure presentation: takes `parts: Array<{ label: string; valueMb: number; color: string }>` and `totalRssMb: number`. Renders a 6 px-tall stacked horizontal bar (rounded 3, sunken background, segment borders 1 px in `--background`) plus a wrap-flex legend below (each item: 8 px square color swatch + label + mono `<%>` + mono `· <MB>` muted).
3. **Wiring.** In `apps/tauri/src/components/diagnostics-pane.tsx`, inside the Memory section's card, after the legend row + `border-top` divider, render the breakdown bar with the four canonical segments. For v1 the values come from a static estimator function (since per-component RSS attribution doesn't exist in `useMemoryStats` today): `breakdownEstimate(rssMb)` returns `{ parakeetMb, audioBuffersMb, appShellMb, cachesMb }` summing to `rssMb`. The estimator: `parakeet = isModelLoaded("parakeet-en") ? 612 : 0`, `audio = 142`, `caches = 84`, `appShell = rssMb - parakeet - audio - caches` (clamped ≥ 100). Lives in `apps/tauri/src/lib/use-memory-stats.ts` next to the existing `externalClaim` helper.
4. **Header.** Above the bar, a flex row: mono kicker `OpenWhisper RSS Breakdown` on the left, mono `<rss-GB> GB resident` on the right. Matches the existing section-header type style.
5. **Caveat inheritance.** No new caveat copy — the existing pane footer already says "system stats are read from the OS once per second" and the per-model RAM caveat (TASK-62.7) covers the attribution wobble. Just add an `aria-label` on the bar describing it as an estimate.
6. **Tokens.** Use existing CSS vars (`--info`, `--background`, `--muted-foreground`, `--font-mono`, `--surface-sunken`). No new tokens.

### Verification

- Playwright (`apps/tauri/tests/diagnostics.spec.ts`, extend or add): launch app → assert bar visible inside Memory card → assert four legend items rendered with the four canonical labels in order → assert percentages sum to ≥ 99 and ≤ 101 (rounding tolerance).
- Manual: launch with no model loaded → assert Parakeet segment is 0 (segment doesn't render) and other three account for the full bar; load Parakeet → assert Parakeet segment appears at the largest size.
- ui-discipline check: bar uses CSS for the segment-rendering, but the *legend* uses shadcn `Badge` or composed text+swatch — not raw `<div>` for primitive duties. Verify no off-spec primitive.

### Outcome ACs

- `<RSSBreakdownBar />` component exists in `apps/tauri/src/components/diagnostics-rss-breakdown.tsx` as a pure-presentation prop-driven component
- Diagnostics → Memory section renders the bar below the existing chart legend, separated by a soft border
- Estimator function `breakdownEstimate(rssMb)` lives in `apps/tauri/src/lib/use-memory-stats.ts` and is unit-tested for: model-loaded case, model-unloaded case, low-RSS clamp
- Legend renders `<segment> <%> · <MB>` for each of: Parakeet weights, Audio buffers, App shell, Caches
- Playwright spec covers presence + percentage-sum invariant
- ui-discipline pass: shadcn primitives where applicable; no styled `<div>` reaching for primitive duties

## Task 2: Settings → Models — Memory budget bar (with hover-ghost preview)

The headline addition. Add a horizontal stacked bar at the top of the Settings → Models pane, anchored to physical RAM, with per-model segments and a hover-ghost preview that drives off model-row hover.

### Steps

1. Read the `openwhisper-ui-discipline` skill before touching `apps/tauri/src/components/`.
2. **Physical RAM source.** Extend `apps/tauri/src-tauri/src/sysmem.rs` (or wherever the existing memory primitive from TASK-62.1 lives — verify) to expose `physical_ram_mb: u64`. Read once at boot via `sysinfo::System::new_all().total_memory() / 1024` (kB → MB). Cache in shell state since physical RAM is static. Add a Tauri command `system_physical_ram_mb() -> u64` if not already present.
3. **Component.** New `apps/tauri/src/components/models-memory-budget-bar.tsx` exporting `<ModelsMemoryBudgetBar />`. Props: `physicalMb: number`, `enabledModels: ModelRow[]`, `owBaseMb: number` (constant 380), `otherAppsMb: number` (computed = `physicalMb - currentSystemUsedMb` from `useMemoryStats`), `previewModel?: ModelRow & { mode: 'add' | 'remove' }`.
4. **Bar rendering.** Same 28 px-tall horizontal bar as the design prototype. Segments left-to-right via `flex: <valueMb> 0 0`: System & other apps · OpenWhisper base · per-enabled model (each in the model's accent color). Headroom is the trailing transparent flex region. When `previewModel.mode === 'add'`: insert a striped (`repeating-linear-gradient`), dashed-bordered, animated (`diag-pulse 1.6s`) ghost segment at `flex: <preview.ramMb> 0 0`, and shrink the headroom flex correspondingly. When `previewModel.mode === 'remove'`: mark the existing model's segment with the same dashed-outline + striped pattern and animate. Use the exact `diag-pulse` keyframe from the prototype's `<style>` block (move it to `apps/tauri/src/index.css` if not already present).
5. **Header readout.** Above the bar: mono kicker `Memory budget` + mono `of <physical> GB physical` on the left; on the right, two stacked or inline values: `OpenWhisper <total> GB` and `Headroom <amount> GB`. When `previewModel` is set, the Headroom value swaps to `<new> (was <old>)` with the new value colored amber (add) or green (remove).
6. **Legend.** Below the bar: one item per segment (color swatch + label + mono value). When previewing an add, append a ghost legend item (`+ <model name>`, striped swatch, value mono). When previewing a remove, the existing model's legend item line-throughs and prepends `− `.
7. **Pane wiring.** In the Settings → Models pane component (locate via `grep -r "Settings.*Models" apps/tauri/src/components/`), add pane-local state `[hoveredModelId, setHoveredModelId]`. Each model-row component gets `onMouseEnter` / `onMouseLeave` handlers that update the state. The budget bar reads the corresponding model from the catalog and passes it as `previewModel` with the right `mode` (`enabled ? 'remove' : 'add'`). When no row is hovered, `previewModel` is undefined and the bar renders its rest state.
8. **Delta chip on the model toggle.** When `hoveredModelId === thisRow.id` AND `!thisRow.enabled`, render a small `+<MB>` chip next to the row's enable toggle (font-mono 10.5 px, padding 1×6, border 1px var(--border), border-radius 999). When the row IS enabled, render `−<MB>` in destructive-leaning tone.
9. **Footer caveat.** Below the bar (and before the model list): the existing info-bg card "Memory figures are projected. Real RSS depends on your inputs..." copy from the design prototype. Wraps a sub-paragraph link "for exact live numbers see Diagnostics → Memory" — the link routes via the same view-state advance the crash entry card uses (no router-level route).

### Verification

- Playwright (`apps/tauri/tests/settings-models.spec.ts`, extend or add): navigate to Settings → Models → assert the bar is the first thing in the pane → hover a disabled model row → assert striped ghost segment appears in the bar AND `+<MB>` chip appears on that row's toggle AND headroom value flips amber → mouse-leave → assert all three revert. Hover an enabled model row → assert dashed/striped pattern on its bar segment AND `−<MB>` chip → assert headroom flips green.
- Manual on macOS: open Settings → Models on a 24 GB machine → assert "of 24.00 GB physical" in the header → toggle a model on → assert bar segment animates in over ~220 ms.
- Manual on Windows: same flow, verify physical RAM matches Task Manager's reported value.
- ui-discipline check: shadcn `Tooltip` for the per-segment hover info, shadcn `Switch` for the toggle (the bar itself is bespoke since shadcn has no equivalent primitive — exempt).

### Outcome ACs

- `system_physical_ram_mb` Tauri command exists and returns the boot-time-cached value
- `<ModelsMemoryBudgetBar />` renders at the top of the Settings → Models pane with segments for system+other apps, OpenWhisper base, and each enabled model
- Hovering a disabled model row reveals a striped/dashed ghost segment in the bar AND a `+<MB>` delta chip on the row's toggle AND swaps the Headroom number to amber `<new> (was <old>)`
- Hovering an enabled model row marks its existing segment as departing (line-through legend, `−<MB>` chip, dashed outline) AND swaps the Headroom number to green `<new> (was <old>)`
- Toggling a model in/out animates the bar segments via the existing `diag-pulse` keyframe over 220 ms
- Footer caveat card is present and links the user to Diagnostics → Memory for live values
- Playwright spec exercises both add-hover and remove-hover paths, plus the rest state

## Task 3: Settings → Models — Storage panel (disk + path + opener)

Add an inline strip below the models list showing total disk used by enabled models, install count, the canonical models folder path, and a Show-in-Finder/Explorer button.

### Steps

1. **Path resolver.** New Tauri command `models_storage_path() -> String` in `apps/tauri/src-tauri/src/lib.rs` (or `apps/tauri/src-tauri/src/models/mod.rs` if that module exists). Resolves `app.path().app_data_dir()? + "models"` at command-time (NOT cached) so that env-overridden config dirs continue to work in tests. Returns the path as a String for direct display in the strip.
2. **Component.** New `apps/tauri/src/components/models-storage-panel.tsx` exporting `<ModelsStoragePanel />`. Props: `enabledModels: ModelRow[]` (uses `model.diskMb` if present, else fallback `model.ramMb * 0.78`); `path: string` (passed down from the pane after the command resolves at mount).
3. **Layout.** Single inline row, padding 12×16, sunken background, rounded 8, border 1px var(--border):
   - Left: mono `<total-GB> on disk` (or `<MB>` if < 1 GB) — accumulator over enabled models' disk weight.
   - Mono dot · `<N> models installed` — count of enabled models.
   - Mono dot · `<path>` — ellipsized via `flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap`.
   - Right: ghost button `Show in Finder` (macOS) or `Show in Explorer` (Windows). Click invokes `tauri-plugin-opener.openPath(path)`.
4. **Pane wiring.** In the Settings → Models pane, render the storage panel below the models list (NOT below the budget bar — between the list and the existing footer caveat). On mount, call `models_storage_path` once and pass the result down.
5. **Platform-conditional copy.** Detect platform via `useEffect` + the existing platform-detection helper (the Diagnostics pane uses one; reuse). macOS → "Show in Finder"; Windows → "Show in Explorer".

### Verification

- Playwright: navigate to Settings → Models → assert storage panel renders with at least the enabled-model count visible and the mono path string present → assert the Show-in-Finder/Explorer button is keyboard-focusable.
- Manual on macOS: click button → Finder opens at the models folder.
- Manual on Windows: same → Explorer opens at the models folder.

### Outcome ACs

- `models_storage_path` Tauri command exists, returns the resolved path at command-time, and is callable from the webview
- `<ModelsStoragePanel />` renders below the model list with disk total, install count, mono path, and a Show-in-Finder/Explorer button
- Button copy switches by platform (`Show in Finder` on macOS, `Show in Explorer` on Windows)
- Click opens the OS file browser at the canonical models path (verified manually on both platforms)
- Playwright spec covers presence + button-focusable invariant

## Sequencing & dependencies

```
Task 1 (RSS breakdown bar)        → can land any time after TASK-62.8 is in-tree
Task 2 (Memory budget bar)        → depends on TASK-62.1 (memory primitive) for live RSS
                                    + new physical-ram resolver added in this task
Task 3 (Storage panel)            → depends on tauri-plugin-opener already in deps
                                    (added by TASK-78.4 for crash folder)
```

The three tasks are otherwise independent. They can interleave with TASK-78 work in either order.

## Cross-plan dependencies

- **TASK-62.7** (Tauri telemetry commands + state-change events) — ships per-model RAM attribution; the Task 1 estimator can later be replaced with the real number once that telemetry is consumable per-component.
- **TASK-62.8** (Diagnostics panel UI) — currently In Review; Task 1 wires *into* its Memory section. If 62.8 ships before this plan starts, Task 1 is a clean addition.
- **TASK-78.3** (Crashes entry card on Diagnostics overview) — referenced from this plan's spec for completeness; the implementation lives there.
- **TASK-78.4** (Detail sheet + Open-folder) — adds `tauri-plugin-opener` to the shell deps if not already present; Task 3 here uses the same plugin.

## Open knobs deferred to implementation

- Exact disk-MB heuristic when `model.diskMb` is missing (today the spec says `ramMb * 0.78`; the catalog may grow a real `diskMb` field by impl time, in which case the heuristic is dead code).
- Whether the budget-bar segment gradient direction matches the model accent color or stays uniform (visual preference; the prototype uses uniform — keep that).
- Whether the storage-panel button uses `tauri-plugin-opener.revealItemInDir` (highlights the folder) vs. `openPath` (opens the folder). Defer to impl-time check of the plugin API surface; the design prototype is silent.
