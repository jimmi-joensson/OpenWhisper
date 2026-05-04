---
id: doc-20
title: 'Pill scales 1.5x during recording / transcribing — spec'
type: spec
created_date: '2026-05-01 00:00'
---

# Pill scales 1.5x during recording / transcribing — spec

**Backlog parent:** TASK-70
**Date:** 2026-05-01
**Revision:** 3 (scale 2× → 1.5× per Mac smoke feedback, 2026-05-02)

---

## Problem

The pill HUD is small (38–70 px wide × 22 px tall logical) so users can lose track of which dictation phase is live, especially on large or HiDPI displays. The visual difference between *idle* (3 dots), *recording* (12 vertical bars), and *transcribing* (rotating sphere) currently relies on subtle pixel-scale shape cues inside a 22 px-tall capsule. On a 32" 4K monitor this reads as "tiny shimmer in the corner".

## Goal

Increase visual presence during the two active phases by scaling the **whole capsule and its content** to 1.5× while idle stays at 1×. The motion must read as **alive, not timed** — the pill is the same UX archetype as Apple's Dynamic Island (status badge that grows when the device is doing something, anchored at a screen edge, lives across all app states), so the size change uses a **spring, not a bezier**: it settles into place with a subtle bounce on grow, snaps decisively on shrink, and carries velocity across interruptions.

## Non-goals

- Not changing the existing inner width tween between recording (70) ↔ transcribing (38) — those values stay; the 1.5× factor multiplies whatever they resolve to.
- Not redesigning particle / sphere geometry — content scales because the capsule is scaled, not because we double every per-particle coordinate.
- Not changing click-through behavior, anchor position, or the dock-gap math (capsule's *bottom edge* must stay where it is today; only the top extends upward).
- Not refactoring the width animation off layout (`style.width`) onto `transform: scaleX()`. That is hygiene, not feel — captured as a finding in TASK-71's audit.
- Not adding a JS animation library (Motion / Framer-Motion / Reanimated). The hand-rolled RAF model already in `PillOverlay.tsx` extends to springs in ~15 lines.

## Behavior model

| From → To | Scale tween | Spring config | Width / pose tween |
| --- | --- | --- | --- |
| idle → recording | 1 → 2 | grow spring (slight overshoot) | 38 → 70 (existing, RAF) |
| idle → transcribing | 1 → 2 | grow spring | 38 → 38, sphere implode/explode (existing) |
| recording ↔ transcribing | stays at 1.5 | (no scale spring fires) | existing |
| recording → idle | 2 → 1 | shrink spring (critically damped, no bounce) | 70 → 38 (existing) |
| transcribing → idle | 2 → 1 | shrink spring | 38 → 38, sphere implode/explode (existing) |

The spring runs on its own clock — it does **not** share `tweenRef.start / duration` with the pose tween. Springs settle when their physics says so, not when a fixed timer expires. Practically the grow spring settles in ~280–340 ms; the shrink spring settles in ~180–240 ms. Both finish well before the existing 520 / 820 ms pose tween, which is the right ordering: the size lands quickly, then the content finishes morphing inside the new shape.

## Decisions

1. **Hand-rolled 2nd-order spring solver in the existing RAF loop.** State per dimension is `{ x, v }` (position, velocity). Each frame:
   ```
   force = (target - x) * stiffness
   v += (force - v * damping) * dt
   x += v * dt
   ```
   Two springs total (one for grow, one for shrink — picked at the moment of status transition based on direction). No library dependency.

2. **Asymmetric spring config.** Apple's `{ duration, bounce }` formulation, hand-translated:
   - **Grow (idle → active):** stiffness `220`, damping `24` → damping ratio ≈ 0.81, ~18% overshoot, settles in ~300 ms. Subtle overshoot reads as "the badge has weight".
   - **Shrink (active → idle):** stiffness `280`, damping `34` → damping ratio ≈ 1.02, critically damped, no overshoot, settles in ~200 ms. Decisive return-to-rest. Matches Emil's "release should always be snappy" rule.

3. **Velocity is preserved across interruption.** If the user cancels mid-grow (status flips from recording back to idle while spring is still extending), the shrink spring inherits the current `v` and rolls forward smoothly — no jolt. This is the headline interruptibility advantage of springs over bezier tweens; bezier-tweens would visibly snap velocity to zero on retarget.

4. **CSS `transform: scale(N)` on `.pill-capsule`**, written each frame from the spring solver. One `style.transform` write per frame; no React state. Layout box stays at unscaled size — only painted pixels are scaled. Width-tween math (`style.width = ${nextWidth}px`) is unchanged.

5. **`transform-origin: 50% 100%`** (center bottom). Bottom edge anchored; growth extends upward. Preserves `place_pill` math — no Rust position changes per state.

6. **Backdrop-filter counter-scale.** At 1.5× the visual blur radius scales with the capsule (20 px → 30 px screen pixels) and the pill's *material* would feel different across states. Counter-scale via CSS custom property: `backdrop-filter: blur(var(--pill-blur))`, RAF writes `--pill-blur = ${20 / currentScale}px`. Net visible blur in screen pixels stays at 20 px constant across the entire scale tween. The capsule's **form** changes; the **material** does not.

7. **`prefers-reduced-motion` honored.** When the media query matches, the spring solver is bypassed: `x` snaps to `target` on every status change (still through the same code path so the rest of the pill — particles, pose, click-through — keeps working). This *also* upgrades the existing pill animation to honor reduced-motion, which it currently does not — we explicitly take that on now because doubling the visual displacement amplifies any vestibular impact.

8. **OS window dimension bump.** `130×82 → 180×110` logical pt. The capsule at 1.5× recording is 105×33; window must contain that plus shadow blur (~14 px) without clipping. Window is transparent so the bump is invisible to the user — it is just a larger paint region. Reposition math constants follow.

## Trade-offs considered

- **Bezier curves with custom cubic-bezier (Emil's `0.23, 1, 0.32, 1`).** Rejected: bezier tweens have fixed durations and zero velocity at the start of every retarget. Springs are the right tool for "feels alive" badges, and the implementation cost (an integrator step in an existing RAF) is trivial.
- **Use a JS spring library (Motion, Framer-Motion, Reanimated).** Rejected: pulls in a runtime dependency for ~15 lines of physics. The pill is a single small component — library overhead would dominate.
- **Scale via JS-tweened `width` + `height` on the layout box.** Rejected: forces paint of SVG and backdrop-filter region every frame. Transform stays on the compositor.
- **Resize the OS window per state.** Rejected: cross-platform window resize during animation is jittery and would need to choreograph with the spring. A larger transparent window once is strictly cheaper.
- **Full Dynamic-Island-style refactor (pin layout at max width, animate `transform: scaleX()` for width too).** Deferred to TASK-71 audit. Scope is bigger than this task — touches existing recording/transcribing width tween, particle x-position math, and clip behavior.

## Open questions

None.
