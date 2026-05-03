---
id: TASK-61.8
title: User-configurable BT resume delay setting
status: Done
assignee: []
created_date: '2026-05-02'
updated_date: '2026-05-03 10:19'
labels:
  - 61-impl
dependencies:
  - TASK-61.4
parent_task_id: TASK-61
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 `BehaviorSettings.bt_resume_delay_ms: u64` field with `serde(default = …)` round-trip from legacy settings.json
- [x] #2 `behavior::BT_RESUME_DELAY_MS` AtomicU64 cache + getter / setter, hydrated at boot from loaded settings
- [x] #3 `behavior_get_bt_resume_delay_ms` / `behavior_set_bt_resume_delay_ms` Tauri commands; setter persists, updates cache, emits `behavior_bt_resume_delay_changed`
- [x] #4 Setter clamps input to [0, 10000] ms server-side so a malformed settings.json or runaway UI never ships an absurd delay to the MediaController
- [x] #5 Windows MediaController reads from cache instead of the const; delay_ms == 0 skips the sleep entirely
- [x] #6 React hook `useBtResumeDelay` mirrors `usePauseAudio` shape (initial invoke + listen for cross-surface event)
- [x] #7 Settings → General → Audio shows a Slider (0–10 s, 500 ms step) below the Pause audio toggle, disabled when the master toggle is off
- [x] #8 Slider description shows live human-readable value ("Wait 5 seconds…" or "Off — music resumes immediately…")
- [x] #9 Playwright tests cover: initial state, Off=0 description variant, cross-surface event update, disabled-when-pause-off
- [x] #10 Existing serde tests updated to assert default = 5000 and bt_resume_delay_ms round-trips
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
### Why a Slider (not a Select)

Considered a Select with discrete presets (1s, 2s, 3s, …, 10s) before reverting to a Slider. Reasons:

1. The user's empirical-tuning pattern — needed 5s after 4s wasn't enough — argues for a continuous control: drag-to-fine-tune is faster than click-list-pick.
2. shadcn's canonical pattern for duration settings is `Field + Slider` with the live value embedded in `FieldDescription` (per [shadcn slider docs](https://ui.shadcn.com/docs/components/slider)). The shape fits exactly.
3. A 0–10 s range with 500 ms step (21 stops) is the sweet spot — fine enough to land on perceptual differences, coarse enough that the slider isn't a thousand-step continuum.

### Slider component

The codebase uses base-ui (`@base-ui/react`) primitives, not Radix, so I added a shadcn-styled wrapper around `@base-ui/react/slider` at `apps/tauri/src/components/ui/slider.tsx`. Single-thumb only. One non-obvious bit: base-ui sets `role=slider` on the Thumb, NOT on the Root, so an `aria-label` spread on Root never reaches the screen-reader-visible element. The wrapper extracts `aria-label` and forwards it to the Thumb explicitly so `getByRole("slider", { name })` and AT both see the same accessible name.

### Files

- `apps/tauri/src-tauri/src/settings/mod.rs` (struct + default + tests)
- `apps/tauri/src-tauri/src/behavior.rs` (cache + getter/setter + commands)
- `apps/tauri/src-tauri/src/lib.rs` (handler registration + boot hydrate)
- `apps/tauri/src-tauri/src/media_control/windows.rs` (read cache instead of const)
- `apps/tauri/src/lib/use-bt-resume-delay.ts` (new hook)
- `apps/tauri/src/components/ui/slider.tsx` (new shadcn-styled wrapper)
- `apps/tauri/src/components/general-pane.tsx` (Slider + dynamic description + formatSeconds helper)
- `apps/tauri/tests/fixtures/tauri-shim.ts` (shim stubs + emit helper)
- `apps/tauri/tests/settings-window.spec.ts` (4 new specs)

### Validation

- `cargo check` clean.
- `pnpm tsc --noEmit` clean.
- Full Playwright suite green (73 / 73 pass), including 4 new specs for this feature.
- Manual smoke pending on user box: drag slider 0 ↔ 10s, verify description updates live, record on BT, verify the wait length matches the slider value.

### Design choices to revisit if user feedback wants

- Step granularity is 500 ms. If users on faster radios want 250 ms granularity (e.g. dial in 1.75 s precisely), drop the step constant.
- Slider width is `max-w-sm` (384 px). If the General pane gets wider in a future redesign, the slider can grow to fill more horizontal room.
- 0 ms is a valid "skip the wait" choice. Users on faster BT radios who'd rather get instant resume + accept the mono blip can dial all the way down.
<!-- SECTION:NOTES:END -->
