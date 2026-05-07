---
name: openwhisper-devtools-sheet
description: Dev-only state-simulation rule — every "park the app in state X for inspection" affordance ships inside the existing DevToolsPanel sheet (`apps/tauri/src/components/dev-tools-panel.tsx`), not as a new floating widget, button, or pane. READ before adding a dev-only override (pill-state picker, fake-crash trigger, simulated stats, "skip the model load" toggle, etc.) for a feature you're building. The trigger lives in the bottom-left of the sidebar; one trigger, one sheet, every dev affordance inside.
---

# DevTools sheet — single home for dev-only overrides

## The rule

If you're building a feature that has a "would be nice to park the app in state X to inspect the UI without producing the state for real" need, the override **goes inside `DevToolsPanel`'s sheet**. Specifically:

- Add a new `<section className="ow-devtools__section">` to `apps/tauri/src/components/dev-tools-panel.tsx`.
- Wire any Tauri commands or React state through props passed from `App.tsx` (where the panel is mounted).
- Reuse the existing CSS classes (`ow-devtools__section`, `ow-devtools__row`, `ow-devtools__cta-row`, `ow-devtools__feedback`, `ow-devtools__code`).
- Gate the entire panel behind `import.meta.env.DEV` at the call site in `App.tsx` — release builds tree-shake it out.

You do **not** add:

- A new floating button or panel anywhere on screen.
- A "dev mode" route in the sidebar.
- A new corner of the app for a one-off override.
- A modal/dialog launched from somewhere other than the DevTools sheet.

## Why

- **Trust the index.** The user knows where dev affordances live. Adding new floating widgets next to the trigger creates a "dev landfill" in the corner — the same anti-pattern the design pivot away from `DevPillControls` was meant to fix (commit history: pre-`/loop` Home pane had a bottom-right pill panel that intercepted real UI).
- **Single z-index story.** The trigger is `z-index: 40` so any open Sheet/Dialog (z-50) covers it. New floating dev surfaces have to repeat that reasoning and usually get it wrong, which is how the crash-detail Delete button started missing clicks.
- **Easy to scope-prune at build time.** One DEV gate at one call site beats many `import.meta.env.DEV &&` checks scattered across feature components. Audit is also one file.
- **Testable.** Playwright fixture stubs need to mock the Tauri commands the dev controls invoke. Funneling all dev commands through one panel keeps the shim's dev-mock surface small and enumerable (`tests/devtools.spec.ts` is the canonical exercise).

## How to apply

When you build a feature that wants a dev override:

1. Open `apps/tauri/src/components/dev-tools-panel.tsx`.
2. Add a new internal section component below the existing `PillControlsSection` / `SimulateCrashSection` pair. Follow the same shape: `<section>` with header (h3 + sub paragraph) + body (rows / CTAs / inline feedback).
3. If the section needs state from above, plumb it via `DevToolsPanelProps` and pass through `App.tsx`. If it's local toggle state, `useState` inside the section is fine.
4. If the section calls a Tauri command, add a stub for that command to `tests/fixtures/tauri-shim.ts` (mirror the `crashes_*` pattern: increment a `__owXxxCount` window prop so a Playwright spec can assert "the button was clicked → the command fired").
5. Add at least one Playwright case to `tests/devtools.spec.ts` exercising the new section.

## Examples (planned + landed)

- **Pill state override** — `PillControlsSection`. Toggles `App.tsx`'s `pillOverride` so the auto-emit stops and a manually-picked phase drives the pill (with the simulated 20 Hz envelope on `recording`).
- **Simulate crash** — `SimulateCrashSection`. Invokes `crashes_debug_trigger_panic` (debug-build-gated Tauri command). The Rust panic hook captures + writes a real crash file, so Diagnostics → Crashes renders against real data instead of the empty state.
- **Future: skip-recognizer-load** — same pattern: a Switch in DevToolsPanel that flips a process-wide `OnceLock<bool>` in core, so you can get to the "ready" pill state without paying the 487 MB Parakeet download on every dev rebuild.
- **Future: simulate stats** — populate `dictations` rows via a `stats_debug_seed` command (similarly debug-gated) so the home pane stats strip has data without an hour of real dictation.

If the feature you're building can be visibly inspected only from a state that's expensive or annoying to produce for real (a big download, a panic, a long recording, a TCC denial), that's exactly the trigger for adding a section here.

## Boundary

This rule does NOT apply to:

- **Production-visible debugging UIs** — diagnostics pane, error banners, crash inspector. Those live where the user expects them, not in the dev sheet.
- **Test fixtures consumed only by Playwright** — those go in `tests/fixtures/tauri-shim.ts` and need no React UI.
- **CLI-only debug paths** — the headless library + CLI surface is the right home for those (per `openwhisper-headless-first`).

It applies specifically to: *in-app, dev-build-only, "park the running app in state X" affordances that you'd otherwise be tempted to drop a floating button or one-off panel for.* Those go in the DevToolsPanel sheet — every time.
