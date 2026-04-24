# OpenWhisper visual identity tokens

Authority spec for colors, type, geometry, and motion that make OpenWhisper recognizable as the same product across macOS, Windows, and (future) Linux.

**Goal**: close + recognizable, not pixel-perfect. Users should pattern-match the HUD pill, the orange recording state, and the tray/menubar state icon on any platform. Platform-native materials (NSVisualEffectView vs Mica/Acrylic) are expected to differ.

**Scope**: tokens only. Behavioral specs (activation UX, hotkey semantics, auto-paste contract) live in `docs/claude-windows-handoff.md`.

---

## 1. Brand color

| Token | Hex | sRGB (0–1) | Usage |
|---|---|---|---|
| `recording` | `#E07000` | `0.88, 0.44, 0.00` | Every "OpenWhisper is armed / recording" indicator: menubar/tray icon, pill level-meter bars, any future record button tint |

**Rule**: this single orange is the only brand color. Don't introduce additional accents for "processing" or "done" states — use type/weight/opacity instead. The orange appearing = a recording is live.

---

## 2. Neutrals and materials

Platform system neutrals (`.secondary`, `.tertiary`, theme foreground) for most UI. Custom opacities only where called out below.

| Surface | Value |
|---|---|
| Pill base fill | Black at **0.55** opacity over the platform HUD blur (`.ultraThinMaterial` on macOS, **Acrylic** on Windows) |
| Pill border | White at **0.08** opacity, **1 px** stroke |
| Idle pill dots | White at **0.4** opacity |
| Idle level-meter bars (inactive state) | Theme secondary at **0.35** opacity |
| Transcript scroll area | Black at **0.2** opacity, **6 px** radius |
| Info / health banner bg | System blue at **0.15** opacity |
| Info / health banner border | System blue at **0.35** opacity, **1 px**, **8 px** radius |
| Stop-button tint | System red |

Window backdrop: Mica (Win 11) / NSWindow default vibrancy (macOS).

---

## 3. Type scale

Platform system font. No bundled custom font.

| Role | Mac (SwiftUI) | Windows (WinUI 3) | Notes |
|---|---|---|---|
| App title / header | `.largeTitle.weight(.semibold)` | `FontSize="32"` `FontWeight="SemiBold"` | Main window header |
| Hint / subtitle | `.callout`, secondary fg | `FontSize="14"` `Opacity="0.7"` | "Right Cmd to toggle" / "Left Ctrl + Space anywhere" |
| Body | `.body` | `FontSize="15"` | Default paragraph text |
| Monospace body | `.system(.body, design: .monospaced)` | `FontFamily="Consolas"` | Transcript + debug values |
| Status (small) | `.caption` (implicit) | `FontSize="12"` `Opacity="0.7"` | Bottom status line |
| Tertiary label | `.foregroundStyle(.tertiary)` | `Opacity="0.55"` | "label:" prefixes in debug panel |

**Font stacks**: San Francisco on macOS (`-apple-system`), **Segoe UI Variable** on Windows 11 (WinUI default), system sans on Linux. Don't force a cross-platform family.

---

## 4. Corner radii

| Surface | Radius |
|---|---|
| Pill | Fully rounded (Capsule — radius = height ÷ 2 = **11 px**) |
| Level-meter bar | **1.5 px** |
| Transcript box | **6 px** |
| Health / info banner | **8 px** |
| Buttons | Platform default (don't override) |

---

## 5. HUD (pill) geometry

| Token | Value |
|---|---|
| Pill outer size | **70 × 22 px** |
| Pill gap above Dock/taskbar | **14 px** |
| Pill horizontal padding (inside) | **8 px** |
| Pill vertical padding (inside) | **5 px** |
| Pill position | Bottom-center of active display |
| Pill HStack item spacing | **4 px** |

### Level meter (inside pill)
| Token | Value |
|---|---|
| Bar count | **12** |
| Bar spacing | **2 px** |
| Bar height (max fill area) | **10 px** |
| Bar min height (at silence) | **3 px** |
| Bar corner radius | **1.5 px** |
| Fill color (recording) | `recording` (`#E07000`) |
| Fill color (inactive / transcribing) | Theme secondary at 0.35 opacity |

### Idle dots
| Token | Value |
|---|---|
| Dot count | **3** |
| Dot size | **3 × 3 px** |
| Dot spacing | **3 px** |
| Dot color | White at 0.4 opacity |

### Main-window level meter (larger variant)
| Token | Value |
|---|---|
| Bar count | **32** |
| Height | **36 px** |
| Same bar geometry as pill otherwise | |

---

## 6. Motion

| Event | Value |
|---|---|
| Level-meter redraw rate | **20 Hz** (50 ms tick) |
| Rolling level-history length | Matches bar count (12 pill / 32 main) |
| Grace return to idle after transcription | **250 ms** |
| State-transition animation | Platform default / none — snap transitions |
| Hover cursor (idle pill) | `pointingHand` / `IBeam` → platform pointing hand |

No custom animation curves. Let each platform's default spring/ease apply. If a future design change calls for explicit curves, add them here first.

---

## 7. Level meter math

Shared dB-normalized amplitude mapping so every platform's meter reads identically for the same audio.

```
floor_db = -55
normalized = clamp( (20 * log10(max(amplitude, 1e-6)) - floor_db) / -floor_db,
                    0, 1 )
```

Reference: `apps/macos/App/LevelMeter.swift:35-38`.

---

## 8. Iconography

| Icon | Source | Notes |
|---|---|---|
| Menubar / tray idle | Mic glyph drawn from `OpenWhisper_Default.svg` geometry | Template on Mac (system-tinted). Mono on Windows (theme-aware). |
| Menubar / tray recording | Same mic glyph, `recording` fill | Non-template on Mac (stays orange). Pre-rendered orange on Windows. |
| App icon | Mic glyph (no brand color) | Used in title bar / dock / taskbar |
| Stop button glyph | `stop.circle.fill` (SF Symbol) / `` (Segoe Fluent) | Shown while recording |
| Record button glyph | `mic.circle.fill` (SF Symbol) / `` (Segoe Fluent) | Shown at idle |

Icon geometry is stored as SVG rects in `apps/macos/App/OpenWhisperApp.swift:242-269` (`StatusIconRenderer.micRects`). Ports should derive from the same source — don't redraw the mic shape per platform.

---

## 9. Per-platform implementation

### macOS (SwiftUI + AppKit)
- `NSColor.openWhisperRecording` + `Color.openWhisperRecording` in `apps/macos/App/OpenWhisperApp.swift:362-370` — canonical brand-color reference.
- Pill values hardcoded in `apps/macos/App/PillOverlay.swift` (`pillSize`, `gapAboveDock`, padding literals).
- Motion values in `apps/macos/App/DictationService.swift:253` (timer) and `apps/macos/App/PillOverlay.swift:182` (grace delay).

**Drift check (2026-04-24)**: values in this spec match the Mac code exactly. Next time this spec changes, also sync Mac source and note the date.

### Windows (WinUI 3)
- Brand color, neutrals, radii, type sizes in `apps/windows/OpenWhisper/App.xaml` merged `ResourceDictionary`.
- Pill / HUD geometry in `PillWindow.xaml` (TASK-24) pulled from `App.xaml` resources — do not hardcode in `PillWindow.xaml`.
- Tray icon assets live under `apps/windows/OpenWhisper/Assets/` — two ICO files (idle template + orange recording).

### Linux (future)
- GTK4 CSS + libadwaita. Brand color as a CSS custom property (`--owh-recording: #e07000`). HUD geometry in a shared CSS file under `apps/linux/data/`.

---

## 10. Source-of-truth rule

**This document is authoritative.** When a value changes:
1. Update this file first.
2. Sync Mac source (likely `OpenWhisperApp.swift` color extension and whichever `*.swift` view owns the geometry).
3. Sync Windows `App.xaml` `ResourceDictionary`.
4. Bump the drift-check date in §9.

Don't change values in platform code without a PR to this file. Drift between platforms is the identity goal's biggest enemy, and it accumulates silently.
