---
id: doc-21
title: 'Spec: Windows custom titlebar (Slack-style) + sidebar continuity'
type: spec
created_date: '2026-05-01 00:00'
---

# Spec: Windows custom titlebar (Slack-style) + sidebar continuity

**Backlog parent:** TASK-68
**Date:** 2026-05-01

## Problem

On Windows the main window shows the **OS-default title bar** (white background, "OpenWhisper Dev", min/max/close) sitting above OW's drawn header strip. Two stacked title rows look broken next to a modern dark Windows app like Slack, Discord, or VS Code, and clash hard against OW's dark body — an instant "Tauri default" tell. Mac has no equivalent issue because `titleBarStyle: Overlay` already merges OS chrome (traffic-light buttons) into our drawn strip.

Visually compounding it: the settings back-arrow titlebar runs full-width above the sidebar, but the sidebar's tinted background (`rgb(0 0 0 / 0.15)`) doesn't match the titlebar's plain `var(--background)`. The seam between them looks like an unfinished gap, especially with the OS bar adding a third color band on Windows.

## Goal

1. Single, dark, app-drawn titlebar on Windows — no OS chrome strip above it. Slack/Discord-class polish.
2. Custom min/max/close controls on Windows that match Win 11 conventions (right-aligned, hover states, restore-icon swap when maximized).
3. Sidebar renders as one continuous column from the very top of the window; the titlebar is **inset over the content area only**, not full-width above the sidebar.
4. Mac stays exactly as it is — `Overlay` mode keeps traffic-light buttons + drawn back-arrow strip identical to today.
5. Aero-snap (`Win+←/→`, snap-assist), Win 11 rounded corners, and standard Windows minimize/close keyboard shortcuts (`Alt+F4`, `Win+↓`) continue to work.

## Non-goals

- Linux chrome (not in MVP).
- Mac titlebar redesign — `Overlay` + traffic-lights stays.
- Theming the titlebar separately from the sidebar (one shared bg).
- Window-level keyboard shortcuts beyond what the OS already gives us.

## Behavior model

```
                 ┌──────────────────────────────────────────────┐
   y=0 ─────────►│ ┌────────┬───────────────────────────────────┤
                 │ │        │  ← Settings              − □ ✕    │
                 │ │  Home  ├───────────────────────────────────┤
                 │ │  Settings                                  │
                 │ │  Diag  │           Pane content            │
                 │ │        │                                   │
                 │ └────────┴───────────────────────────────────┘
                 └──────────────────────────────────────────────┘
                  ↑sidebar    ↑titlebar inset over content only
                   from y=0   ↑min/max/close right-aligned (Windows)
                              ↑on Mac: traffic lights at top-left of sidebar
```

Sidebar column starts at `y=0` and runs full window height. Titlebar is a `36px` strip rendered **inside** the content column only — it carries the back-arrow + Settings title (when in Settings) and on Windows the WindowControls cluster on the right. On Mac the WindowControls slot stays empty; AppKit draws the traffic-light triplet over the sidebar's top-left corner (same way Slack does it).

## Decisions

### D1 — Windows decorations dropped via Rust `cfg`, not config

`tauri.conf.json` keeps `decorations: true`. We drop decorations only on Windows in `src-tauri/src/lib.rs::setup()` behind `#[cfg(target_os = "windows")]` via `window.set_decorations(false)`. Reason: avoids per-platform branching in two config files (`tauri.conf.json` + `tauri.dev.conf.json`) — the dev overlay's `app.windows[]` array replaces (not merges), so platform-conditional decorations would have to be duplicated four times.

### D2 — Aero-snap relies on `WS_THICKFRAME` staying set

Tauri 2's `set_decorations(false)` keeps `WS_THICKFRAME` on the window style. That preserves Aero-snap (Win+arrows, edge snap, snap-assist). We do not strip the style ourselves.

### D3 — Win 11 corner radius from DWM defaults

Win 11 applies `DWMWA_WINDOW_CORNER_PREFERENCE = DWMWCP_DEFAULT` to top-level windows automatically, which renders as rounded corners (`8px` radius). We do not call `DwmSetWindowAttribute` ourselves. If we land this and corners are square on Win 10, we accept it — the customer base is Win 11.

### D4 — WindowControls drawn in React, not in Rust

The min/max/close buttons live in `apps/tauri/src/components/window-controls.tsx`. They invoke Tauri 2's `getCurrentWindow().minimize() / toggleMaximize() / close()` via the JS bridge. Rendering is gated on `/win/i.test(navigator.platform)` so the slot collapses to nothing on Mac. The maximize icon swaps to a restore icon by subscribing via `getCurrentWindow().onResized(cb)` (Tauri 2's window-scoped resize hook — **not** the global `listen("tauri://resize")` which doesn't fire reliably for synthetic resize events) and re-reading `isMaximized()` on each callback.

The maximize/restore glyphs are **hand-rolled SVGs** (single rounded square for maximize; two overlapping squares for restore), not lucide icons. lucide-react does not ship a Win 11 "restore-down" glyph (`Copy` is a clipboard icon, not the OS chrome glyph), and the Win 11 chrome convention is specific enough that pulling shapes from a generic icon set looks off. Two ~12×12 inline SVGs keep the cluster tight and match `Square`'s stroke weight.

### D5 — Sidebar column extends to `y=0` on both platforms

The grid restructures so the sidebar is the leftmost column from window top to window bottom; the titlebar lives **only** inside the content (right) column. On Mac, the sidebar gets a `padding-top` of `38px` so its first item clears the traffic-light overlay zone (3 buttons × ~14 px circle + ~12 px AppKit padding ≈ 36 px; round up to 38). This matches Slack's macOS layout. **One number across spec/plan/AC** — 38 px, no other value.

### D6 — Drag region stays on the inset titlebar strip — descendants opt in individually

`data-tauri-drag-region` continues to live on the inset titlebar element (now inside the content column). Per the existing macOS gotcha (drag.js does **not** walk ancestors — see `openwhisper-platform-gotchas` skill, "Window drag silently no-ops" entry), every descendant inside the titlebar that should be draggable needs its own `data-tauri-drag-region` attribute. The back button + WindowControls buttons take `data-tauri-drag-region="false"` so their `onClick` still fires; the title `<h1>` and the cluster's empty area carry `data-tauri-drag-region`. The sidebar column is **not** a drag region — clicking sidebar items must toggle routes, not initiate window drag.

## Trade-offs

| Approach | Pro | Con |
|---|---|---|
| **`decorations: false` (chosen)** | Single dark bar, Slack-class look | Maintain min/max/close ourselves; rely on Tauri's `WS_THICKFRAME` retention |
| `titleBarStyle: "Transparent"` | Less custom code | Mac gotcha (loses focus-loss blur on traffic-lights) per `openwhisper-platform-gotchas` |
| Native OS bar with theming | No custom controls | No way to recolor the OS bar to match dark theme on Windows; mismatched look guaranteed |

## Risk register

- **R1**: `set_decorations(false)` strips `WS_THICKFRAME` on some Tauri versions → Aero-snap dies. Mitigation: verify on Win 11; fall back to a `windows-rs` call to re-add `WS_THICKFRAME` if needed (one-line escape hatch documented in the platform-gotchas update).
- **R2**: WindowControls cluster overlaps content scrollbars at narrow widths. Mitigation: cluster is in the titlebar strip's `flex` row, scrollbars are inside `.ow-app__body` which sits below. Test at min window width (`600x500`).
- **R3**: Mac traffic-lights collision with sidebar's top items. Mitigation: D5's `36px` padding-top on sidebar (Mac-only via `body[data-platform="macos"]`).
- **R4**: Playwright runs on Linux/Chromium where `navigator.platform` is `"Linux x86_64"`, neither Mac nor Windows. WindowControls won't render in tests by default — that's fine for Mac-shape assertions; need a dedicated test that **forces** the Windows branch via `addInitScript` overriding `navigator.platform` if we want to assert Windows control behavior.

## Out of scope (explicit follow-ups)

- Custom titlebar context menu (right-click on title gives "Close / Minimize" etc.) — Windows defaults work.
- Snapping the pill window — pill is a separate window with its own chrome story.
- Windows 10 corner-radius polyfill — Win 11 only target.
