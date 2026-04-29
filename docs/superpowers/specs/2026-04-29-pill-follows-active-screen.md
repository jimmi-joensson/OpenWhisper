# Pill follows active screen — design

**Backlog parent:** TASK-55
**Date:** 2026-04-29
**Status:** Spec → Plan

## Problem

The pill HUD is positioned bottom-center of "its current monitor" once at boot and never moves again. On multi-monitor setups the user routinely loses sight of it: focus moves to a window on display 2, but the pill is parked on display 1. The dictation lifecycle (start, mic levels, transcript injection) keeps working, but the user has no visual feedback in the screen they are looking at.

## Goal

The pill should sit on whichever monitor hosts the currently-focused application. When the user focuses an app on a different monitor, the pill jumps to that monitor (bottom-center, same recipe as today). Behavior is on by default — a Settings toggle lets power users opt out.

## Non-goals (this spec)

- **Manual drag.** Out of scope. A future spec will add per-monitor persisted positions and a drag affordance; that work is gated behind shipping the auto-follow first.
- **Animated transition** between monitors. The pill teleports. Cross-monitor animation requires platform-specific compositing tricks and isn't worth the cost for a 500 ms-cadence event.
- **Per-monitor saved positions.** Not until drag ships. Auto-follow always lands at bottom-center, identical recipe across monitors.

## Behavior model

- Detection runs on the existing 500 ms poll thread in `apps/tauri/src-tauri/src/fullscreen/mod.rs`. One foreground query per tick already happens for fullscreen detection; we derive a second signal (focused-window monitor) from the same query.
- "Active monitor" = the monitor whose rect contains the centre of the focused window. Tracked as an `(i32, i32)` origin tuple. Stable across queries on a fixed display arrangement; changes only when displays are physically rearranged.
- When the active-monitor origin changes between two consecutive ticks (and the new value is `Some`), the watcher invokes a callback that repositions the pill window to bottom-center of the new monitor.
- Recording and transcribing do not gate the follow behavior. The pill jumps mid-recording. The SVG-particle RAF loop is unaffected because `set_position` operates on the window, not the WebView's rendering pipeline.
- Cooldown / debounce: none. 500 ms is already the floor; rapid alt-tab across monitors will ping-pong the pill at most twice per second. If feel-testing later shows this is annoying, we will add a debounce — not before.

## Skip cases (watcher returns `None`, pill stays put)

| Case | Reason |
|---|---|
| No focused window | Nothing to follow. |
| Focused process is OpenWhisper itself | Don't chase our own focus changes. Same skip already used for fullscreen detection. |
| macOS shell / mac Finder no-window state | AX query returns no `AXFocusedWindow`. |
| Win shell classes (`Progman`, `WorkerW`, `Shell_TrayWnd`, `Shell_SecondaryTrayWnd`) | Already filtered by the existing fullscreen helper; reuse the same skip list. |
| Foreground app is fullscreen | Existing fullscreen handler hides the pill. No reposition needed on a hidden window. |
| AX permission revoked on macOS | Watcher returns `None` — pill stays where it is. Same fail-safe as fullscreen detection. |

## Cross-platform implementation

| Concern | macOS | Windows |
|---|---|---|
| Foreground window | `AXUIElementCreateSystemWide → AXFocusedApplication → AXFocusedWindow` (already in tree, used for fullscreen detection) | `GetForegroundWindow` (already in tree) |
| Window rect | `kAXPositionAttribute` (CGPoint) + `kAXSizeAttribute` (CGSize) | `GetWindowRect` (already in tree) |
| Monitor enumeration | `CGGetActiveDisplayList` + `CGDisplayBounds` — thread-safe, callable from any thread. **Not** `NSScreen.screens` (main-thread only). | `MonitorFromWindow(MONITOR_DEFAULTTONEAREST)` + `GetMonitorInfoW` (already in tree) |
| Threading | Watcher thread does the AX + CG calls directly. The Tauri `set_position` call is dispatched onto the main thread via `app.run_on_main_thread` to play safe across both platforms. | Same — `set_position` via main thread. |
| Permissions | Already granted; AX trust is required for the hotkey path. No additional prompt. | None required. |

## Settings shape

```jsonc
// settings.json — additive, sibling of `hotkeys` and `audio`.
{
  "pill": {
    "follow_active_screen": true
  }
}
```

- Field is optional. Absent = treat as `true` (zero-config principle: lead with auto-detect, only add explicit config as an override).
- Setter writes the JSON file AND flips a process-global `AtomicBool` so the watcher reads the new value on its next tick — no restart required.
- Toggle UI lives in Settings → General, alongside the upcoming "Open at Login" entry (TASK-54). Label: "Follow active screen". Helper text: "Pill jumps to whichever screen has the focused app."

## Trade-offs / open decisions

- **Per-monitor positions deferred.** Confirmed with user: if drag ships, positions will be per-monitor; until then, no storage needed.
- **Follow during recording: yes.** The pill is most valuable when visible to the user, and `set_position` does not disturb the level-meter RAF loop. No carve-out for recording state.
- **Toggle in General pane vs new Pill pane: General.** Single setting; doesn't warrant its own pane.
- **Cooldown: none in v1.** 500 ms poll is the floor; revisit only if rapid switching feels jarring.

## Risks

- **Mac AX position attribute returns top-left of the focused window in global display coordinates.** Confirmed against Apple AX docs — origin matches `CGDisplayBounds` origin. Both originate from the primary display's top-left.
- **Monitor identity by origin tuple is sufficient when displays do not move.** If the user re-arranges displays mid-session, the watcher's `LAST_MONITOR` cache is stale by one tick — pill jumps once on the next foreground change, which is acceptable.
- **`available_monitors()` enumeration on macOS.** Tauri's implementation may go through `NSScreen.screens` internally; we therefore wrap the lookup-and-set call inside `app.run_on_main_thread` rather than calling it from the poll thread directly.

## References

- Existing watcher backbone: `apps/tauri/src-tauri/src/fullscreen/mod.rs`, `…/mac.rs`, `…/windows.rs`.
- Current static-position command: `position_pill_bottom_center` in `apps/tauri/src-tauri/src/lib.rs:326`.
- Project principles applied: zero-config-over-toggles (lead with auto-on), orchestration-in-rust (position decision lives in Rust shell, UI is dumb).
