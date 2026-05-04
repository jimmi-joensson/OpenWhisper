---
id: doc-13
title: 'Settings → General pane scaffold — design'
type: spec
created_date: '2026-04-29 00:00'
---

# Settings → General pane scaffold — design

**Backlog parent:** TASK-56
**Date:** 2026-04-29
**Status:** Spec → Plan
**Source design:** `SettingsGeneralBoard` in the Claude Design bundle (`screens.jsx:332`), tokens in `tokens.css`. Local fetch under `/tmp/ow_design/openwhisper/`.

## Problem

`apps/tauri/src/Settings.tsx:99` renders `<PaneStub title="General" />` — a placeholder with no controls. Two upcoming feature tasks (TASK-54 Open at Login + theme stub, TASK-55.6 Follow active screen) each assume a real General pane exists; both currently say "drop the row into the pane that doesn't exist yet". We need to scaffold the pane itself first, then those feature tasks become single-row wiring tasks rather than pane-creation tasks.

## Goal

Build the General pane chrome — sections, rows, toggles — using **shadcn primitives** (per project convention, established in commit `bc4b81b` which added the shadcn skill). The pane should match the design's visual language but render only rows whose features are either already shipped or have a live Backlog task; rows for unscheduled features (Show in Dock, Sound effects, Check for updates) are deliberately omitted so we don't ship dead UI.

## Non-goals (this spec)

- **Launch-at-login wiring.** Owned by TASK-54. We render a placeholder `Switch` here; TASK-54 binds it to `tauri-plugin-autostart`.
- **Theme behavior.** Per TASK-54 AC#4, theme is a stub renders-only. We build the `ToggleGroup`; no theme-application logic.
- **Follow-active-screen toggle.** Owned by TASK-55.6. We do not pre-stub that row here; TASK-55.6 adds it as part of its own row-wiring step.
- **Refactoring AudioPane / ShortcutsPane** to use the new shadcn primitives. Possible later cleanup; out of scope for the scaffold.

## Components used (all from shadcn)

Per the shadcn skill's "use existing components first" rule and component-selection table:

| Need | shadcn component |
|---|---|
| Boolean toggle | `Switch` |
| 2–5 mutually exclusive options (Theme: System / Light / Dark) | `ToggleGroup` + `ToggleGroupItem` |
| Row layout (label + hint + control) | `FieldGroup` + `Field` + `FieldLabel` + `FieldDescription` |
| Section divider | `Separator` |
| Future "Check for updates" button | `Button` (already installed) — out of scope here |

Currently installed (per `pnpm dlx shadcn@latest info --json`): `alert`, `button`, `card`. Need to add: `switch`, `toggle-group`, `field`, `separator`.

Project context (relevant fields):

- **Framework:** Vite + React (`isRSC: false` → no `"use client"` directive needed).
- **Tailwind:** v4 with `@theme inline` block in `apps/tauri/src/App.css`.
- **Style / base:** `radix-nova` / `radix` — uses `asChild` for custom triggers.
- **Icon library:** `lucide` (no icons needed for this scaffold; sidebar already renders emoji glyphs).
- **Aliases:** `@/components`, `@/components/ui`, `@/lib/utils`.

## Visual treatment

The design's `Toggle` paints info-blue (`--info: #0A84FF`) when on. shadcn's `Switch` paints `--primary` (currently neutral grey in this project's `radix-nova` style). To honor the design without violating the shadcn rule "no manual `dark:` color overrides / no `className` color overrides on components":

- **Customize via CSS variable** in `App.css`. shadcn's `Switch` exposes its checked-state background through Tailwind's `data-[state=checked]:bg-primary`. We add a `[data-slot="switch"][data-state="checked"]` rule (or scoped to the General pane) that overrides the background to `var(--info)`. This is the customization-via-CSS-variables path documented in `customization.md`, not a per-instance className override.
- Alternative considered and rejected: pass `className="data-[state=checked]:bg-[var(--info)]"` on each `Switch`. Works, but violates the spirit of the styling rule and scatters the design decision across call sites.

Section structure follows the design's three-column layout: 180px label column, 1fr control column, 14px vertical row padding, 1px top border per row. shadcn's horizontal `Field` orientation is the closest match; verify its built-in spacing during implementation, override grid columns only via `className` (layout-only, allowed per styling rule).

## Sections rendered (this scaffold)

| Section | Rows |
|---|---|
| **Startup** | Launch at login (Switch, **placeholder** — local state only, no persistence; TASK-54 binds it) |
| **Appearance** | Theme (ToggleGroup with `system` / `light` / `dark` items, **stub** — local state only, no theme-application logic per TASK-54 AC#4) |
| **Updates** | Current version (live — `invoke<string>("core_version")` rendered in mono font using `font-mono` token) |

Sections explicitly **not** rendered:

- "Show in Dock" — no Backlog task; macOS-only (`NSApp.setActivationPolicy`) and not justified for the scaffold.
- "Sound effects" — no Backlog task; would require new core feature (start/stop tones).
- "Check for updates" — no Backlog task; auto-update infra is part of TASK-37 and not yet shipped.
- "Follow active screen" — owned by TASK-55.6, stays out of this scaffold.

## Risks

- **Token leakage between panes.** The Switch override targets `[data-slot="switch"]` globally. If a future pane wants a non-info Switch we need to scope the override. v1: global is fine (only Switches we ship are in the General pane).
- **Tailwind v4 `@theme inline` block already exists** in `App.css` — verify the Switch override doesn't fight existing theme directives during implementation.
- **TASK-54 / TASK-55.6 plan deltas.** Both pre-existing tasks/plans say "create the General pane structure" implicitly. After this scaffold lands, those tasks reduce to row-wiring. We update TASK-54's dependency line during plan setup; TASK-55.6's plan markdown gets a one-line note pointing at the GeneralPane file.

## References

- Design source: `/tmp/ow_design/openwhisper/project/screens.jsx:332` (`SettingsGeneralBoard`), tokens at `tokens.css`.
- shadcn skill: `.agents/skills/shadcn/SKILL.md` + `rules/styling.md` + `rules/forms.md`.
- Existing pane patterns: `apps/tauri/src/Settings.tsx` (AudioPane line 137, ShortcutsPane line 382 — both use BEM classes; new pane uses shadcn primitives, no BEM).
- Project-principles: `openwhisper-project-principles` skill — "zero-config over toggles, lead with auto-detect" justifies omitting dead rows.
