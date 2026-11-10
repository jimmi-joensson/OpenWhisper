---
id: TASK-92
title: Windows pill — solid fill (replace backdrop-filter, idle GPU stays active)
status: To Do
assignee: []
created_date: '2026-05-14'
labels: []
milestone: m-1
dependencies: []
priority: high
---

## Description

Field report: with OpenWhisper running and otherwise idle, a user's Windows GPU stays in a non-idle power state continuously. Mac Activity Monitor shows the same pattern as elevated Energy Impact (~10× peer apps) — the Windows symptom is the GPU side of the same root cause.

Root cause for the Windows GPU draw: `apps/tauri/src/PillOverlay.css:51` — the pill capsule uses
`backdrop-filter: blur(var(--pill-blur, 20px)) saturate(140%)`, and the pill window is `transparent: true` + `alwaysOnTop: true` (`apps/tauri/src-tauri/tauri.conf.json:32-34`). On Windows / WebView2 this combination forces DWM to re-sample the framebuffer behind the pill through the blur+saturate kernel on every compositor frame — whenever anything behind the pill repaints (cursor movement, taskbar clock seconds, any window's blinking caret), the GPU has to redo the kernel. The GPU never reaches its lowest P-state. The `will-change: width, transform` on `.pill-capsule` also pins the layer GPU-resident.

On macOS the same CSS is OS-accelerated (WebKit hands `backdrop-filter` to Core Image / system vibrancy) and is cheap, so the existing visual stays.

**Product constraint (explicit):** do NOT hide the pill on Windows in idle. The pill being present at rest is part of the OpenWhisper feel; the fix is to make it cheaper to render, not to remove it.

The CSS file already contains the exact endpoint we want, gated behind `@media (prefers-reduced-transparency: reduce)` (currently used for RDP):

```css
.pill-capsule {
  -webkit-backdrop-filter: none;
  backdrop-filter: none;
  background: rgba(20, 20, 20, 0.92);
}
```

This task routes Windows through that branch unconditionally.

## Implementation sketch

1. Add a platform marker on the pill webview root. Two viable shapes — pick one:
   - **JS:** in `PillOverlay.tsx`, detect Windows via `/win/i.test(navigator.platform)` (matches the convention already used in `App.tsx` for the Windows-keyboard path) and toggle a class like `platform-win` on the pill root.
   - **Rust/Tauri:** emit `cfg!(target_os = "windows")` into the pill webview via an `initialization_script` that sets `document.documentElement.dataset.platform = 'win'`. Cleaner because it avoids the runtime UA sniff and matches one place.
2. In `PillOverlay.css`, scope the existing `backdrop-filter` + `box-shadow` rules to `:not(.platform-win) .pill-capsule` (or `:where(html:not([data-platform="win"])) .pill-capsule`), and unconditionally apply the reduced-transparency endpoint on Windows: `backdrop-filter: none`, `background: rgba(20, 20, 20, 0.92)`.
3. In `PillOverlay.tsx`, skip the RAF write of `--pill-blur` on Windows — the counter-scale exists only to keep blur radius constant across the 1×↔2× scale tween, which is irrelevant when blur is off. (Don't drop the whole RAF; the spring scale tween, particle morph, and width tween still need it. Just gate the `--pill-blur` setProperty.)
4. Leave `will-change: width, transform` for now — needed for the scale/width tween. Re-evaluate after #3.

## Acceptance Criteria

- [ ] On Windows, `getComputedStyle(.pill-capsule).backdropFilter === 'none'` (verify via WebView DevTools, or inspect against a Playwright snapshot if one is added).
- [ ] On Windows, the pill is still visible at rest (`pill.hide()` behavior unchanged — no new hide-on-idle).
- [ ] On Windows idle (no recording, app running for ≥60 s), Task Manager → Performance → GPU shows GPU engines drop to idle or to a noticeably lower utilization vs. `main` baseline. Capture before/after screenshots and attach to the PR.
- [ ] On macOS, the pill's translucent vibrancy is visually unchanged (manual diff against `main`).
- [ ] On macOS, no new behavior or regression in pill RAF / scale tween / particle morph.
- [ ] `prefers-reduced-transparency: reduce` still routes to the same solid endpoint on macOS RDP and on any other host that sets the preference.
- [ ] Playwright suite passes (`pnpm test:ui`) — UI tests that snapshot the pill don't regress.

## Out of scope (separate tasks)

These were identified in the same investigation but are deliberately not bundled here:

- `dictation_tick` emitter @ 20 Hz unconditional emit (`apps/tauri/src-tauri/src/lib.rs:403`) — gate on `phase == Recording`. Separate task.
- Pill RAF empty-deps `useEffect` runs forever (`apps/tauri/src/PillOverlay.tsx:243-491`) — cancel when spring settled. Separate task.
- ORT `Session` held in memory after warmup (`core/src/recognizer/ort_parakeet.rs:49`) — give recognizer a finite idle-drop timeout on Windows so DirectML doesn't pin the GPU device context. Separate task — biggest single Windows GPU win after this one.

## Reference

- Symptom report: friend's Windows machine, OpenWhisper running, GPU consistently shows as active in Task Manager.
- Existing reduced-transparency fallback (which this task reuses on Windows): `apps/tauri/src/PillOverlay.css` `@media (prefers-reduced-transparency: reduce)` branch.
- Milestone gate: blocking [m-1] (`v1.0 — public release readiness`).
