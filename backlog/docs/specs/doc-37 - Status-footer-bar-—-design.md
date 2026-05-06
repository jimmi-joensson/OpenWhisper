---
id: doc-37
title: Status footer bar — design
type: specification
created_date: '2026-05-06 05:10'
---

**Backlog parent:** TASK-86
**Mock:** screenshot from 2026-05-06 design pass (Home pane empty state, footer row at bottom of window).

## Problem

Today the main window has a sidebar + content column with a titlebar inset in the content column only. There is no persistent surface that tells the user *"the app is ready, your hotkey is bound, this is the engine you're talking to"*. That information lives only in the Diagnostics pane, which is a debugging surface, not chrome. Result: a user opening the window cannot tell at a glance whether dictation is wired up — they must press the hotkey and observe the pill to find out.

## Goal

Add a persistent **status footer** spanning the full window width so every pane (Home, Settings, Diagnostics) shows the same one-line status. Three regions, left to right:

1. **Settings hint** (in the sidebar column): `⌘, SETTINGS` — keyboard shortcut to open Settings, also clickable.
2. **Engine + phase status** (left of the content column): `● Ready · Parakeet · on-device`. Status dot color reflects the dictation phase.
3. **Hotkey display** (right of the content column): `Hotkey  Right ⌥` — current toggle hotkey rendered as a keycap.

This collapses three "is the app working?" questions into chrome the user never has to navigate to.

## Non-goals

- No clickable hotkey rebind from the footer (Settings already owns rebind UI). Footer is read-only for the hotkey.
- No engine swap from the footer (engine choice is a Settings concern; footer reflects current).
- No multi-line, no mobile/narrow-window collapse — main window has a fixed-ish min width.
- No animation on the status dot (steady color, no pulse). The pill HUD already does motion; the footer is calm chrome.
- No history, no stats, no transcript content. Stats are TBD (separate ticket).
- No internationalization for "Ready" / "Recording" / "Transcribing" — English-only matches the rest of the shell.

## Behavior model

### Layout

The current shell renders:

```
ow-app
└── ow-app__shell    (flex row)
    ├── SidebarNav
    └── ow-app__column
        ├── ow-titlebar (inset, content-column-only)
        └── ow-app__body
```

The footer must span the full window width including the sidebar column. So it sits as a sibling **after** `ow-app__shell`:

```
ow-app
├── ow-app__shell      (flex row, fills 1fr)
│   ├── SidebarNav
│   └── ow-app__column …
└── ow-app__footer     (full width, fixed height)
```

`ow-app` becomes `flex flex-col h-screen`; the shell takes `flex-1`; the footer is a fixed-height bar. No layout shift in panes — the footer height is reserved by the column layout.

### Sections

**Left — Settings hint.** Lives inside a region whose width matches the sidebar (so the hint visually aligns under the sidebar nav). Renders shadcn `Kbd` for `⌘,` followed by uppercase muted-foreground "SETTINGS". Click target = whole region; click navigates to `route="settings"` (same effect as the existing global `⌘,` keyboard shortcut at App.tsx:98–108). On Windows, the `⌘` glyph becomes `Ctrl` and the shortcut is `Ctrl+,` (matches existing platform-aware hotkey rendering).

**Center — engine + phase status.** Three text fragments separated by middot characters (`·`):

1. **Status dot + phrase** — small circle (`size-2 rounded-full`) followed by phrase derived from `dictation.phase`:
   - `PHASE_IDLE` → green dot, "Ready"
   - `PHASE_RECORDING` → red dot, "Recording"
   - `PHASE_TRANSCRIBING` → amber dot, "Transcribing"
   - `PHASE_DONE` (transient) → green dot, "Ready" (treat same as idle for footer purposes)
   - `PHASE_ERROR` → red dot, "Error"
2. **Engine name** — e.g. "Parakeet". Comes from a new `recognizer_info` Tauri cmd (see "Rust surface").
3. **Origin** — "on-device". Constant for now (every recognizer we ship is on-device); reading from Rust keeps the door open for cloud engines later without a UI change.

Color tokens: dot uses `bg-emerald-500`, `bg-red-500`, `bg-amber-500` (Tailwind defaults map to semantic enough for status); text uses `text-muted-foreground` for separators and origin, `text-foreground` for engine + phase phrase.

**Right — Hotkey display.** Lives in the right portion of the content column, padded to mirror the sidebar's left padding for visual symmetry. Label "Hotkey" in `text-muted-foreground` followed by the toggle hotkey rendered with shadcn `Kbd`. Hotkey string comes from the existing `useCurrentHotkey("toggle")` + `formatHotkeyLabel` pair (already used by `home-pane.tsx:30`). Reactive: rebinding in Settings updates the footer in the same tick.

### Empty / fallback states

- **Hotkey unbound** (rare — startup race or user cleared it): right region shows label "Hotkey" + `Kbd` with em-dash content `—`.
- **Engine name unavailable** (recognizer not yet loaded): center region collapses to dot + phrase only — no separator, no engine, no origin. As soon as `recognizer_info` resolves, the engine + origin appear.
- **Phase = ERROR**: dot turns red, phrase = "Error". The HealthBanner stack inside HomePane still shows the actionable message; footer just acknowledges the state.

### Interactions

- Hover on Settings hint region: subtle `bg-accent/40` background, cursor pointer.
- Hover on hotkey region: no interaction (read-only). No hover state.
- Status group: no interaction. Pure indicator.
- Footer is always visible — it does not hide on Settings or Diagnostics panes.

## Rust surface

One new Tauri command, no events:

```rust
#[tauri::command]
fn recognizer_info() -> RecognizerInfo;

struct RecognizerInfo {
    name: String,    // e.g. "Parakeet"
    origin: String,  // "on-device" | (future: "cloud")
}
```

Backed by a method on the existing `Recognizer` trait (`core/src/recognizer/mod.rs`) — each impl returns its display name and origin. `FluidAudioBridge` and `OrtParakeet` both return `("Parakeet", "on-device")` for now. Engine name does not change at runtime (no hot-swap), so React fetches once on mount and caches.

## Why these choices

**Footer outside `ow-app__shell`, not inside the content column.** The screenshot's `⌘, SETTINGS` hint sits visually under the sidebar nav, not under the content. Putting the footer inside the content column would either misalign that hint or require a hack to reach across columns. Sibling-after-shell with the footer's own internal grid (sidebar-width + 1fr) keeps both regions self-explanatory.

**Engine name in Rust, not hardcoded in React.** Per the `openwhisper-orchestration-in-rust` skill: status strings live in core. Even though "Parakeet" is currently the only value, having the React side hardcode it would silently lie the moment we add a second engine. A trivial `recognizer_info` cmd costs nothing now and avoids a refactor later.

**No animation on the status dot.** The pill HUD owns motion in this app (`openwhisper-animation-philosophy`). Calm chrome means the dot is a steady color — if it pulsed, users would parse it as "something is happening" and look for the pill. Steady color = "this is current state, nothing to do".

**shadcn `Kbd`, not custom keycap CSS.** Keeps the keycap styling consistent with future Settings-pane usage and benefits from shadcn's font + border tokens. `Kbd` is not currently in the project's `apps/tauri/src/components/ui/` — first task in the plan adds it.

**Status text is the phase, not the `statusMessage` from Rust.** `dictation.statusMessage` carries developer-facing strings ("transcribing on ANE…", "no audio captured"). The footer wants the user-facing phase noun. Mapping the small enum in React keeps copy decisions in the place that owns the UI.

## Risks

- **Sidebar-width sync.** The Settings-hint region must match the sidebar's width or the hint slides under the content column. Sidebar width is set in CSS — the footer must read the same custom property (`--ow-sidebar-width`) rather than duplicate the constant. If the sidebar is ever made resizable, the hint would need a CSS subscription.
- **Tauri 2.10 drag region.** The current titlebar uses `data-tauri-drag-region` so the user can drag the window from the top. If the footer accidentally inherits that, clicking the Settings hint would start a window drag. Footer must explicitly opt out where needed.
- **Hotkey rebind race.** After a successful rebind, `useCurrentHotkey` re-fetches via the `settings_get_hotkeys` command. If the footer reads from the same hook, React will re-render — verify there is no flash-of-old-key.
