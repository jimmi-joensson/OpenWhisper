---
name: openwhisper-animation-philosophy
description: READ before adding or changing any animation, transition, hover, press feedback, or motion-bearing interactive element in OpenWhisper. Triggers - pill HUD shape/state changes, record-button or icon press/hover, mount/unmount transitions, settings pane switches, hotkey activation feedback, model-load placeholders, success/celebration states, sidebar nav active states. Also fires when the user asks "should this animate?" or "make this feel more alive."
---

# OpenWhisper Animation Philosophy

Two source skills + one rule. The rule decides which source applies.

- **emil-design-eng** (third-party, installed at `~/.agents/skills/emil-design-eng/SKILL.md`) — restraint, springs for "alive" badges, <300 ms UI animations, asymmetric grow/shrink, ban on keyboard-action animation.
- **Josh W Comeau's articles** (https://www.joshwcomeau.com/animation/, demos at https://whimsy.joshwcomeau.com/) — physics-based springs, squash & stretch, particle effects, decorative whimsy, "spark joy".
- **Rule:** the *frequency tier* of the surface picks the source. Never pick by personal taste, never apply one philosophy globally.

## Frequency tiers

| Tier | Surfaces | Frequency | Mode | Permitted motion |
|---|---|---|---|---|
| T1 | pill HUD, hotkey activation, paste-injection | 50–100×/day | Emil-strict | State-indication only (e.g. pill grow). Spring for shape change. **No** animation on the action itself. Reduced-motion: snap. |
| T2 | record-button, sidebar-nav active state, transcript-row entry, button hover/press, window-controls hover, mic-glyph | tens/day | Emil + sparing icon polish | `:active scale(0.97)` press feedback. Hover gated behind `(hover: hover) and (pointer: fine)`. Icon micro-animations OK if subtle (≤150 ms). No idle/looping motion. |
| T3 | home-pane mount, settings pane switch, dialogs, model-load placeholder, diagnostics expand, health-banner | occasional | Emil + light Josh | Mount transitions (fade + small translate, ≤8 px). Stagger lists 30–80 ms. Tab-style clip-path color transitions. Spring on container morph. |
| T4 | onboarding flow, first-launch, milestone success states (first dictation, model fully loaded), celebrations | rare / first-time | Josh permitted | Squash & stretch, particle bursts, sparkles, decorative whimsy. **Reduced-motion still required** — fallback to opacity-only. |

When in doubt about tier, **demote one**. T2-with-whimsy reads as noise. T1-with-whimsy reads as broken.

## OpenWhisper surfaces — explicit assignments

(Settles future debates. Update when new surfaces are added.)

- **T1**: `apps/tauri/src/PillOverlay.tsx`, global hotkey indicator, paste/typing injection (invisible — no motion).
- **T2**: `record-button.tsx`, `sidebar-nav.tsx`, `transcript-row.tsx` (entry only), `window-controls.tsx`, `mic-glyph.tsx`, all button hover + press states.
- **T3**: `home-pane.tsx` mount, settings pane switch, `health-banner.tsx`, `diagnostics-pane.tsx` reveals, model-load placeholder (TASK-64).
- **T4**: First-launch onboarding (none today — reserve), milestone success states (none today — reserve).

## Rules every tier obeys

- **Animate `transform` and `opacity` only.** Width / height / margin / padding / top / left trigger reflow. Banned without an explicit perf trade-off logged in the task spec.
- **`prefers-reduced-motion: reduce`** snaps motion-transforms to target. Opacity + color transitions still run — they aid comprehension, not vestibular load.
- **Hover behind a media query**: `@media (hover: hover) and (pointer: fine)` — false positives on touchscreens / RDP otherwise.
- **No `transition: all`.** Name properties.
- **Animation state in refs, never React state.** RAF writes DOM directly. (Project-wide rule from `feedback_animation_refs_not_state` memory.)
- **RAF + refs** for sustained / coordinated animation. **CSS transitions** for one-shot hover, press, mount.
- **Asymmetric timing** (Emil): press slow & deliberate, release fast & decisive. Default ratio ~3:1.
- **Custom curves over built-ins.** UI-feel: `--ease-out: cubic-bezier(0.23, 1, 0.32, 1)`. "Alive" / shape morph: spring (hand-rolled 2nd-order solver — see `PillOverlay.tsx` for the pattern).
- **Cohesion across tiers.** A T4 surface adjacent to a T2 surface should not feel like a different app. Match the slower-tier cadence to the surrounding feel.

## Icon micro-animations (cross-cutting, mostly T2)

Icons are the cheapest "feel alive" lever. Josh's whimsy.joshwcomeau.com builds an entire language out of `:active`, `:hover`, and state-change motion on SVG. OpenWhisper uses lucide icons throughout (commit `c01e125`). Default lucide is static. Make them respond.

Patterns (apply at the tier shown):

- **Press feedback** (every clickable icon, T2): `transform: scale(0.92)` on `:active`, `transition: transform 120 ms cubic-bezier(0.23, 1, 0.32, 1)`. Cap 150 ms.
- **Hover lift** (T2 nav icons only): subtle 1–2 px translateY or `scale(1.05)` on hover, gated behind `(hover: hover)`. Skip for primary action icons (record button — keep serious).
- **Stroke-draw on enter** (T3 dialogs / panel mounts): animate `stroke-dasharray` + `stroke-dashoffset` to draw the icon line ~400 ms ease-out, fires once on mount.
- **State morph** (toggle icons, T2): stage two paths; crossfade with `clip-path` or opacity over 180 ms. Don't interpolate the SVG `d` attribute — janky in WebKit.
- **Celebration burst** (T4 only): Canvas overlay particles (not 50 SVG elements). Fires on milestone, never on routine success.

**Per-icon rule:** does the icon convey a state change? If yes (play↔pause, mic-on↔mic-off), animate the morph. If no (decorative or affordance-only), animate only press feedback.

**Rejected examples** (calibrate by knowing what doesn't fit):

- ❌ Spinning gear on settings hover — T2 surface, motion implies "loading", false signal.
- ❌ Squash on record-button press — T2, but the action is hotkey-adjacent and the pill already communicates state. Plain `scale(0.97)` is enough; squash is too much.
- ❌ Sparkles on every paste-success — would fire 50×/day. Move to milestone-only.
- ✅ Mic-glyph rotates 8° on `:active` — T2, single tactile cue, no false signal.
- ✅ Back-arrow in settings panes slides 2 px left on hover — T2, navigational hint.

## Decision flow

Before writing animation code:

1. **Which tier?** (table above) — if unclear, demote one.
2. **What's the purpose?** state-indication / feedback / preventing-jarring-change / spatial-consistency / decorative. T1+T2 reject "decorative". T3+T4 accept it.
3. **Spring or duration?**
   - "Alive" / shape morph / interruptible → spring (hand-rolled in RAF; pattern in `PillOverlay.tsx` scale tween).
   - One-shot fade / slide / press → CSS transition + custom-bezier.
   - Constant motion (progress, marquee) → `linear` keyframe.
4. **Reduced-motion fallback?** Required. No exceptions.
5. **Performance:** transform + opacity only? If not, log the trade-off in the task spec.

## Cross-references

- Full Emil framework: `~/.agents/skills/emil-design-eng/SKILL.md` (don't duplicate — read directly when needed).
- Josh's articles index: https://www.joshwcomeau.com/animation/ (read the relevant article when reaching for a Josh technique).
- Spring solver pattern: `apps/tauri/src/PillOverlay.tsx` + `backlog/docs/specs/2026-05-01-pill-active-scale.md`.
- Animation-in-refs rule: `feedback_animation_refs_not_state` memory entry.
- Pill geometry constants: `project_pill_geometry` memory entry.
