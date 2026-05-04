---
id: doc-9
title: 'Pill scales 1.5x during recording / transcribing — implementation plan'
type: plan
created_date: '2026-05-01 00:00'
---

# Pill scales 1.5x during recording / transcribing — implementation plan

**Backlog parent:** TASK-70
**Spec:** backlog/docs/specs/2026-05-01-pill-active-scale.md
**Date:** 2026-05-01
**Revision:** 3 (scale 2× → 1.5× per Mac smoke feedback, 2026-05-02)

Each `### Task N:` heading maps 1:1 to a Backlog subtask `TASK-70.N`. Four tasks; sequence: 1 → 2 → 3 → 4. Tasks 2 and 3 can be sequenced 2 → 3 (spring solver first, then reduced-motion + blur counter-scale on top) but must not be re-ordered: the reduced-motion branch lives inside the spring update path.

---

### Task 1: Bump pill OS window dimensions + reposition math

**Goal.** Give the WebView paint region enough headroom for the 1.5× capsule (105×33) plus its `0 4px 14px` shadow without clipping. Capsule's idle on-screen Y must stay put. (Window dims `180×110` were sized for an earlier 2× target and are kept — over-sized transparent margin is free; under-sized would clip.)

**Files.**

- `apps/tauri/src-tauri/tauri.conf.json` — pill window `width: 130 → 180`, `height: 82 → 110`.
- `apps/tauri/src-tauri/tauri.dev.conf.json` — same pill entry. Per `feedback_tauri_dev_overlay_windows` memory, this file *replaces* (does not merge) `tauri.conf.json`'s `windows[]`, so every pill key has to be mirrored.
- `apps/tauri/src-tauri/src/lib.rs` `place_pill` — update `PILL_WIN_W`, `PILL_WIN_H` constants to match. `CAPSULE_BELOW_PAD = (PILL_WIN_H - CAPSULE_H) / 2.0` recomputes automatically. The `y = work_area_bottom - gap - PILL_WIN_H + CAPSULE_BELOW_PAD` solve algebraically holds the *idle* capsule's bottom at `work_area_bottom - gap` for any `PILL_WIN_H` (the +pad term cancels the −PILL_WIN_H term). Confirm by reading the comment block at lines 594–619.

**Steps.**

1. Edit `tauri.conf.json` pill window entry → `width: 180, height: 110`.
2. Edit `tauri.dev.conf.json` to mirror — copy the full pill block, not just changed keys.
3. Edit `src-tauri/src/lib.rs` `place_pill` constants `PILL_WIN_W` and `PILL_WIN_H` to `180.0` and `110.0`.
4. Build + dev-run on Mac (`scripts/dev-run.sh`, per `feedback_tauri_mac_hardened_runtime` and the TCC pain memory — *not* `pnpm tauri dev`).
5. Trigger an idle-only state (no recording yet). Compare the capsule's screen Y to a baseline screenshot from before the change. Should match within ±1 logical pt.
6. Build + dev-run on Windows. Same idle-position check. Verify the capsule clears the taskbar by `ABOVE_DOCK_GAP` (24 logical pt).

**Outcome ACs (Backlog).**

- Pill window in both `tauri.conf.json` and `tauri.dev.conf.json` is 180×110 logical pt.
- `PILL_WIN_W` / `PILL_WIN_H` in `src-tauri/src/lib.rs` `place_pill` match the conf values.
- Idle capsule's on-screen position matches the pre-change baseline within ±1 logical pt on both Mac and Windows.
- App boots, pill appears, no clipping artefacts at the new window edges.

---

### Task 2: Spring-driven scale tween in PillOverlay

**Goal.** Capsule scales 1× ↔ 1.5× via a hand-rolled 2nd-order spring solver written from the existing RAF loop. Asymmetric: subtle overshoot on grow, critically damped on shrink.

**Files.** `apps/tauri/src/PillOverlay.tsx`, `apps/tauri/src/PillOverlay.css`.

**Steps.**

1. In `PillOverlay.tsx`, add the status → scale map and spring configs:
   ```ts
   const PILL_SCALE: Record<PillStatus, number> = {
     idle: 1,
     recording: 2,
     transcribing: 2,
   };
   const SPRING_GROW = { stiffness: 220, damping: 24 };   // ~18% overshoot
   const SPRING_SHRINK = { stiffness: 280, damping: 34 }; // critically damped
   ```
2. Add `scaleStateRef = useRef<{ x: number; v: number }>({ x: 1, v: 0 })`. This replaces the bezier-driven `scaleRef` from revision 1.
3. Track previous frame timestamp in a ref (`prevTickRef = useRef<number>(0)`) so the integrator uses real `dt`. Clamp `dt` to `[0, 1/30]` (33 ms) so a tab-throttled gap doesn't blow up the spring.
4. In the RAF `tick`, after the existing pose-tween block, add the spring step:
   ```ts
   const targetScale = PILL_SCALE[statusRef.current];
   const grow = targetScale > scaleStateRef.current.x;
   const cfg = grow ? SPRING_GROW : SPRING_SHRINK;
   const dt = Math.min(1 / 30, Math.max(0, (now - prevTickRef.current) / 1000));
   prevTickRef.current = now;
   const s = scaleStateRef.current;
   const force = (targetScale - s.x) * cfg.stiffness;
   s.v += (force - s.v * cfg.damping) * dt;
   s.x += s.v * dt;
   // Snap when both displacement and velocity are below visible thresholds.
   if (Math.abs(targetScale - s.x) < 0.0005 && Math.abs(s.v) < 0.005) {
     s.x = targetScale;
     s.v = 0;
   }
   if (capsuleRef.current) {
     capsuleRef.current.style.transform = `scale(${s.x.toFixed(4)})`;
   }
   ```
5. Note: the spring picks `cfg` per-frame based on whether it is currently above or below the target. This naturally handles the interruption case (status flipped back to idle while still growing): the next frame sees `targetScale < x`, switches to `SPRING_SHRINK`, but velocity is preserved in `s.v` so the motion continues smoothly into the shrink with current momentum.
6. In `PillOverlay.css`, on `.pill-capsule`:
   - Add `transform-origin: 50% 100%;` so growth extends upward from the bottom edge.
   - Extend `will-change: width` → `will-change: width, transform`.
7. **Do not change `.pill-root` layout.** The existing `align-items: center` + `CAPSULE_BELOW_PAD` math from Task 1 already places the *unscaled* idle capsule's bottom at `work_area_bottom - gap`. `transform: scale(N)` with `transform-origin: 50% 100%` does not affect layout — only paint — so the post-transform bottom edge stays at that same Y regardless of scale.
8. Smoke-run via `scripts/dev-run.sh` on Mac. Verify:
   - Hold hotkey: capsule grows with a barely-perceptible overshoot at the top of the motion.
   - Release: scale stays at 1.5× through transcription (no re-trigger from recording → transcribing because both are `targetScale = 1.5`).
   - Transcription completes: capsule shrinks decisively, no overshoot.
   - **Interruption test:** start recording, then within ~150 ms cancel the hotkey. Capsule should reverse smoothly without snapping.

**Outcome ACs (Backlog).**

- `PILL_SCALE` map, `SPRING_GROW` / `SPRING_SHRINK` configs, and `scaleStateRef` (`{ x, v }`) exist in `PillOverlay.tsx`.
- RAF tick computes spring step with real `dt` (clamped to ≤33 ms) and snaps to target when displacement < 5e-4 and |velocity| < 5e-3.
- `.pill-capsule` has `transform-origin: 50% 100%` and `will-change: width, transform`.
- Manual smoke on Mac: idle → recording shows subtle overshoot; recording → idle is critically damped (no overshoot).
- **Interruption smoke on Mac:** retargeting mid-spring carries velocity through the direction reversal — no visible jolt.
- Manual smoke on Windows: same behavior, no clipping at window edges (relies on Task 1).

---

### Task 3: Reduced-motion fallback + backdrop-filter counter-scale

**Goal.** Honor `prefers-reduced-motion`. Keep the pill's *material* (backdrop-blur) visually invariant in screen pixels across the scale.

**Files.** `apps/tauri/src/PillOverlay.tsx`, `apps/tauri/src/PillOverlay.css`.

**Steps.**

1. Add `prefersReducedMotionRef = useRef<boolean>(window.matchMedia("(prefers-reduced-motion: reduce)").matches)`.
2. Subscribe to changes: `mediaQueryList.addEventListener("change", ...)` → updates the ref. Clean up on unmount.
3. In the spring-step block from Task 2, branch at the top:
   ```ts
   if (prefersReducedMotionRef.current) {
     scaleStateRef.current.x = targetScale;
     scaleStateRef.current.v = 0;
     // Still write the transform so first frame after toggle picks up.
   } else {
     // ...existing spring step...
   }
   ```
   Same code path runs after the branch (transform write + custom-prop write from step 5 below) so reduced-motion users still get the snap, just instantly.
4. Apply the same reduced-motion branch to the existing **width** RAF tween (around line 380–390, the `tweening && tw.from` block): when `prefersReducedMotionRef.current`, snap `widthRef.current = targetWidth` and skip interpolation. This upgrades the existing pill animation to honor reduced-motion — explicitly in scope per spec decision 7.
5. Backdrop-filter counter-scale. In `PillOverlay.css`:
   ```css
   .pill-capsule {
     /* ...existing... */
     -webkit-backdrop-filter: blur(var(--pill-blur, 20px)) saturate(140%);
     backdrop-filter: blur(var(--pill-blur, 20px)) saturate(140%);
   }
   ```
   In the RAF tick, after writing `transform`, write the counter-scaled blur:
   ```ts
   if (capsuleRef.current) {
     capsuleRef.current.style.setProperty(
       "--pill-blur",
       `${(20 / Math.max(s.x, 0.001)).toFixed(2)}px`,
     );
   }
   ```
   At scale 1: `20px`. At scale 2: `10px`. Net visible blur in screen pixels stays at `20px × scale = 20px` constant. Cap denominator at 0.001 to defend against arithmetic edge cases (the spring should never go below ~1.0 in practice).
6. Smoke-run on Mac with reduced-motion **on** (System Settings → Accessibility → Display → Reduce motion). Verify status changes are instant — no spring, no width tween. Particles still morph (their per-state shape is information, not motion-for-motion's-sake).
7. Smoke-run on Mac with reduced-motion **off** at scale 1 vs scale 1.5. Open a window with a high-contrast bright background behind the pill; the blur disk visible *through* the capsule should look the same diameter at both scales. If it visibly grows at 1.5×, the counter-scale wasn't applied.

**Outcome ACs (Backlog).**

- `prefersReducedMotionRef` subscribes to media-query changes and unsubscribes on unmount.
- Reduced-motion branch snaps `scaleStateRef.x` and `widthRef.current` to target instantly (no spring, no width tween) — but particle pose tweens still run.
- `.pill-capsule` uses `var(--pill-blur, 20px)` for `backdrop-filter` and `-webkit-backdrop-filter`.
- RAF writes `--pill-blur` per frame as `20 / scale`, denominator clamped at 0.001.
- Manual smoke: with reduced-motion enabled, status changes are instant; with reduced-motion disabled, blur disk is visually constant across scale 1 ↔ scale 2.

---

### Task 4: Playwright spec + cross-platform smoke

**Goal.** Lock the scaled capsule dimensions in a regression test, and confirm the spring + blur behavior on both platforms.

**Files.** `apps/tauri/tests/pill-overlay.spec.ts` (new), `apps/tauri/tests/fixtures/tauri-shim.ts` (extend if needed to emit `pill_state` events).

**Steps.**

1. Add a Playwright spec that mounts the pill route. Mirror the existing `settings-window.spec.ts` for how a non-main window is mounted under the dev server.
2. Use `tauri-shim` to emit `PILL_STATE_EVENT` with `{ status: "recording", levels: [...] }` and similar for transcribing / idle.
3. After dispatching a status change, wait for the spring + width to settle (~600 ms is comfortable headroom — spring settles by ~340 ms grow / ~240 ms shrink, plus the longest pose-tween path of 820 ms for sphere transitions). Then read `capsuleRef`'s bounding rect via `element.getBoundingClientRect()`.
4. Assert (within ±1 px to absorb sub-pixel rounding):
   - idle: rect ≈ 38 × 22.
   - recording: rect ≈ 105 × 33.
   - transcribing: rect ≈ 57 × 33.
   - `getBoundingClientRect` returns the *post-transform visual* rect in Chromium / WebKit — that is exactly what we want.
5. **Reduced-motion assertion.** Set `page.emulateMedia({ reducedMotion: "reduce" })`. Dispatch idle → recording. The capsule should reach its 105×33 visual rect within ~500 ms (no spring, no tween).
6. Run `pnpm exec playwright install chromium` (per repo CLAUDE.md), then `pnpm test:ui`.
7. Manual smoke on **both** platforms (per `openwhisper-task-lifecycle` — feature isn't done until verified on the surfaces it ships on):
   - Mac: hold hotkey, observe spring grow with subtle overshoot; release, observe snap shrink (no overshoot). Interruption test: start recording, immediately cancel — motion reverses smoothly.
   - Windows: same. Particularly check Windows RDP (`prefers-reduced-transparency: reduce` branch) — capsule should still scale, just without backdrop blur.

**Outcome ACs (Backlog).**

- `pill-overlay.spec.ts` exists and passes locally via `pnpm test:ui`.
- Spec asserts capsule visual dimensions for all three states within ±1 px.
- Spec asserts reduced-motion path: status change reaches target rect within ~50 ms.
- Mac manual smoke: spring grow with subtle overshoot, snap shrink with no overshoot, smooth interruption reversal.
- Windows manual smoke: same behavior, no clipping, RDP reduced-transparency branch still scales.
- No regressions in existing pill-related smoke flows (record → transcribe → paste round-trip still works).
- Existing pill width / sphere tween cadence visually unchanged at 1× idle (no per-frame transform write fires when status is steady-idle, `s.x === targetScale`, and `s.v === 0`).
