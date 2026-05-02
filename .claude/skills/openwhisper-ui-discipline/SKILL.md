---
name: openwhisper-ui-discipline
description: UI work discipline rule — every UI change loads the `shadcn` skill first, and every settings/option control choice (slider vs select vs input vs toggle group) is researched against shadcn-canonical patterns before committing. READ before editing any file under `apps/tauri/src/components/` or `apps/tauri/src/lib/use-*.ts`, before adding a new control or settings row, before picking a UI primitive, or before reaching for a styled `<div>` instead of a shadcn component. Applies to React components, settings panes, hooks, layouts, slider/select/input/toggle decisions, and anything that changes what the user sees on screen.
---

# UI work discipline

Two rules, both earned by real session-level mistakes.

## Rule 1: Load the `shadcn` skill before touching UI

When a user request involves React components, settings panes, layouts, controls, or any file under `apps/tauri/src/`, invoke the `shadcn` skill **before** writing or editing code. This loads the project's shadcn rules (base-vs-radix differences, FieldGroup/Field patterns, semantic colors, no `space-y-*`, `data-icon` for button icons, etc.) so the change lands shadcn-correct on the first commit instead of after a review cycle.

**Why.** This codebase is shadcn + base-ui (not Radix). The skill carries non-obvious rules — base-ui Select needs an `items` prop, base-ui Slider accepts plain numbers (Radix needs arrays), `cn()` for class merging, semantic tokens like `text-muted-foreground` instead of raw `text-gray-*`. Without the skill loaded, you write Radix-shaped code that compiles but violates conventions, then has to be refactored.

**How to apply.**
- Before editing any `.tsx` file under `apps/tauri/src/components/` or `apps/tauri/src/components/ui/`, invoke `Skill(skill: "shadcn")`.
- Same for new hooks under `apps/tauri/src/lib/use-*.ts` that wire UI state.
- Same for new shadcn-style component wrappers (e.g. adding `slider.tsx` next to `select.tsx`).
- After loading, follow its rules: `FieldGroup`+`Field` instead of `<div className="space-y-*">`, `flex gap-*` instead of `space-y-*`, `cn()` for conditional classes, `tabular-nums` for digit-stable readouts, semantic colors only.
- Skip when the change is genuinely UI-irrelevant: a Tauri command in Rust, a backlog file, a test fixture, etc.

## Rule 2: Research the right control type before committing to one

When adding a settings row, a configuration input, or any user-tunable value, **don't default to the first control that comes to mind** (usually Select). Match the control to the data shape:

- **Slider** for continuous numeric ranges where empirical tuning matters — durations, opacities, thresholds, anything where the user might want "a bit more" or "a bit less" between testing iterations.
- **Select** for genuinely discrete enumerations — theme={"system","light","dark"}, language, device, model. If you find yourself manufacturing presets ("1s, 2s, 3s, …") to fit a continuous range into a Select, you wanted a Slider.
- **ToggleGroup** for 2–7 mutually-exclusive options that benefit from being all-visible-at-once (theme picker, view mode).
- **Switch** for binary on/off.
- **Input** for free-form text; `Input type="number"` only when sliders/steppers don't fit (e.g. wide range with arbitrary precision).
- **NumberField (stepper)** for small integer adjustments where ±1 nudging is the primary interaction.

**Why.** A Settings row that ships as a Select when it should have been a Slider gets the user request "change this to a Slider, fine-tuning 4s vs 5s vs 6s is exactly the use case" within one review cycle. That's a wasted iteration and a wasted PR. The control choice IS part of the design — match it to the use case before writing the JSX.

**How to apply.**
- For any new settings/preference row, before writing JSX: state the data shape (continuous vs discrete vs binary), state the user's tuning pattern (set-once vs revisit-frequently vs nudge-by-one), then pick the control.
- Cross-check against the shadcn skill's `Component Selection` table — it lists which primitive maps to which need.
- For continuous numeric settings: prefer **Slider with live value readout + endpoint labels** (the shadcn `Field + Slider` pattern). Match the value-readout style to the project's existing pattern (in OpenWhisper today: `<span className="font-mono text-sm">` mirroring the `Current version` row in General pane).
- Don't embed the live value INSIDE the description text — sentence-level changes ("Wait 2 seconds…" → "Off — music resumes immediately…") cause the description block to reflow and visibly jump as the user drags. Put the value in a stable label slot (right-aligned baseline-aligned to the FieldLabel, or under the slider track).
- If genuinely unsure between Slider and Select, ask the user — don't ship the wrong one and force a follow-up.

## Boundary

This rule does NOT apply to:

- Backlog file edits, plan documents, README changes — no rendered UI involved.
- Rust-side changes in `apps/tauri/src-tauri/` — those don't touch the WebView.
- Pure logic refactors in `apps/tauri/src/lib/` that don't change rendering or hook surfaces.
- Test files (`apps/tauri/tests/`).
- Updating an existing control's bound value/onChange — keep the existing control type; both rules apply to *introducing* or *replacing* a control.

## What counts as "before"

Both rules are pre-flight checks. The right time to invoke `shadcn` and to think through the control type is **before the first JSX tag is written**. Mid-implementation course-correction is fine — but if the user has to ask "did you consider a slider?" or "is this following shadcn?", that's a process miss, not a creative-disagreement question.
