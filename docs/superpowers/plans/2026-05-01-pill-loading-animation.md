# Pill loading-state animation — implementation plan

**Backlog parent:** TASK-64
**Date:** 2026-05-01
**Depends on:** TASK-63 (placeholder location must exist)

Each `### Task N:` heading maps 1:1 to a Backlog subtask `TASK-64.N`. Small UX polish — three tasks total. Sequence: 1 → 2 → 3.

This plan does not have a separate spec. The shape is small and the design rationale lives inline below.

---

## Design rationale (lieu of spec)

**Problem.** TASK-63 ships a placeholder loading indicator (text "Preparing cleanup…" or static dot) in the pill while a model is in `Loading` state. It works but doesn't visually match the existing pill states (idle dots, recording bars, transcribing spinner). Mac is the source of truth for visual identity (per `openwhisper-project-principles`); we extend the pill's existing visual vocabulary.

**Goal.** Replace the placeholder with an animation that:

- Uses the existing identity tokens (recording orange `#E07000`, pill background, level-meter geometry).
- Is visually distinct from the recording-bars and transcribing-spinner states (so users can tell "loading model" apart from "transcribing").
- Respects `prefers-reduced-motion` — falls back to a static visual for users with that preference.
- Lives in the existing pill component surface so the TASK-63.9 placeholder swap is a one-line change.

**Non-goals.** Cross-monitor jump animation, sound effects, haptics. Animation runs entirely in the WebView; no compositor tricks.

**Open question for execution.** Mac SwiftUI's `PillOverlay.swift` may need a parallel implementation depending on whether macOS is on the Tauri shell or still on the SwiftUI shell at the time of execution. If both shells are live, ship the Tauri version first; the Mac SwiftUI shell follow-up is a separate task. (As of the plan date, the SwiftUI shell may already be retired per TASK-41 — verify at execution time.)

---

### Task 1: Animation design + identity-token alignment

**Goal.** Pick the visual shape — likely a slow pulsing dot, a progressive-fill bar, or a "wave" of dots cycling through the pill — and align with `docs/design/identity-tokens.md`. Document the choice and motion specs (duration, easing).

**Files.** `docs/design/identity-tokens.md` (append "Loading state" subsection), no code changes.

**Steps.**

1. Read `docs/design/identity-tokens.md` for the existing pill state vocabulary. Note recording-orange `#E07000`, pill geometry (70×22, per memory), existing state shapes (idle dots, recording bars, transcribing spinner).
2. Pick the loading visual. Recommendation: **breathing dot** — single 6-px dot, pill-foreground color, opacity 0.4 ↔ 1.0 over 1.6 s. Visually distinct from idle (multiple static dots) and transcribing (rotating spinner).
3. Document the motion spec: duration, easing (`ease-in-out` recommended), color reference, dot placement, reduced-motion fallback (static dot at 1.0 opacity).
4. Append a "Loading state" subsection to `docs/design/identity-tokens.md`.
5. Verify with the user before implementation if the visual choice is non-obvious — this is a design decision, not just code.

**Outcome ACs (Backlog).**

- "Loading state" subsection added to `docs/design/identity-tokens.md`.
- Visual is distinct from existing idle / recording / transcribing states.
- Motion spec includes duration, easing, color, reduced-motion fallback.

---

### Task 2: Implement the animation in the pill component

**Goal.** Build the React component for the loading visual; swap in for the placeholder from TASK-63.9.

**Files.** `apps/tauri/src/PillOverlay.tsx` (or wherever the pill states render), possibly a new `apps/tauri/src/components/pill-loading-state.tsx`.

**Steps.**

1. Find the placeholder added in TASK-63.9 — the spec says it lives in a single replaceable location.
2. Implement `PillLoadingState` as a React component. Use CSS animations or Reanimated-style refs (per `feedback_animation_refs_not_state` memory: animation state in refs, never React state).
3. Honor `prefers-reduced-motion` via `@media (prefers-reduced-motion: reduce)` — render the static fallback.
4. Replace the TASK-63.9 placeholder with `<PillLoadingState />`.
5. Verify visually in dev build: trigger a cold cleanup load by waiting 90 s after the previous dictation, then dictating again. The pill should show the breathing dot during the load, then transition to recording / transcribing as usual.
6. `pnpm exec tsc` clean.

**Outcome ACs (Backlog).**

- `PillLoadingState` component exists and renders the design from Task 1.
- Replaces the TASK-63.9 placeholder; the placeholder code path is removed.
- `prefers-reduced-motion` falls back to the static visual.
- Manual smoke: cold cleanup load shows the new animation in the pill.

---

### Task 3: Playwright snapshot + reduced-motion test

**Goal.** Cover the new component with a snapshot test and a reduced-motion assertion.

**Files.** Extend `apps/tauri/tests/pill.spec.ts` (or wherever pill UI tests live; create if absent).

**Steps.**

1. Test: with `model-state-changed` shim emitting `{ label: "cleanup-llm", state: "Loading" }`, the pill renders `PillLoadingState`. Snapshot the rendered HTML.
2. Test: with the same shim plus `prefers-reduced-motion: reduce` set on the page, the static fallback variant renders. Snapshot.
3. Test: when `state` transitions to `Loaded`, `PillLoadingState` unmounts and the regular pill state takes over.
4. `pnpm test:ui` green.

**Outcome ACs (Backlog).**

- Snapshot test for animated state passes.
- Snapshot test for reduced-motion fallback passes.
- Unmount-on-transition test passes.
- `pnpm test:ui` green.

---

## Reviewer loop

Three tasks; no separate spec. Run plan-document-reviewer with the standard criteria + Backlog enforcement addendum.

## Execution handoff

Strictly sequential: 1 (design) → 2 (implementation) → 3 (test). Total surface is small enough to execute in one sitting once TASK-63 is shipped.
