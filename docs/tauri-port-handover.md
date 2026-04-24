# Tauri port — handover prompt

Paste this (or the relevant sections) to a fresh Claude session or hand it to
a collaborator. Self-contained: a reader cold to the project should be able
to start from here.

---

## 1. What OpenWhisper is

OpenWhisper is an open-source local-dictation app — Superwhisper-class
(press hotkey, talk, tap again, transcript is pasted into the focused app).
Parakeet-TDT model (NVIDIA, CC-BY-4.0) runs on-device: CoreML via
FluidAudio on Mac, ONNX Runtime + sherpa-onnx on Windows / Linux.

MIT license. Local dictation is free by default and must never be paywalled.
Paid tiers only justified for things that cost the project to run (hosted
sync, managed billing). Stack decisions follow from those product values.

Code layout today:

```
core/                   Rust crate — orchestration, state machine, audio capture
                        (cpal), VAD, post-processing, config, custom vocab,
                        C ABI exposed for shells
apps/macos/             Swift + SwiftUI/AppKit shell. SHIPPED. Source of truth.
apps/windows/           C# + WinUI 3 shell. Abandoned in favor of Tauri (kept
                        in git as reference).
docs/                   claude-windows-handoff.md (prior architectural brief),
                        design/identity-tokens.md (visual spec)
backlog/                Backlog.md CLI task tracking
```

## 2. Why we're pivoting to Tauri

The Windows WinUI 3 shell shipped working functionality (pill HUD, tray,
hotkey, dictation, text injection, fullscreen-aware behavior) but its
visual quality didn't match Mac closely enough. The pill capsule, material,
and overall feel read as "different app" despite shared design tokens. User
compared to SuperWhisper (cross-platform, uniform look on Mac and Windows)
and decided consistency > native feel.

Prior stance ("native UI per OS") is reversed. New direction:

- **Tauri 2** as the shell (Rust backend + WebView frontend)
- **React + TypeScript** for UI
- **Tailwind CSS + shadcn/ui** (Radix-based components) for styling
- **Existing Rust core** linked directly (Cargo path dependency — no FFI
  crossing inside Tauri since it's already Rust)
- **Mac SwiftUI stays shipped** as the source of truth for behavior and
  visuals. Tauri mirrors it. Mac may migrate to Tauri later if/when the
  Tauri version meets or exceeds parity.

Chose Tauri over Electron because: Rust-native fits our existing core;
smaller binary footprint matters for a menubar/tray utility; security model
(no Node runtime) fits a local-first tool. Chose shadcn/ui because components
are owned (copied into repo, not imported) and Radix primitives give
accessibility for free.

## 3. Mac is the source of truth

When in doubt about behavior or visuals, read `apps/macos/App/` and mirror.
Files to study in order:

1. `apps/macos/App/OpenWhisperApp.swift` — app entry, tray icon, menu
   structure, lifecycle.
2. `apps/macos/App/PillOverlay.swift` — signature visual (pill HUD states,
   positioning, click-through, grace-return).
3. `apps/macos/App/DictationService.swift` — orchestration flow (the phase
   machine the UI reacts to).
4. `apps/macos/App/HotkeyService.swift` — Mac activation (Right Cmd
   tap-not-hold). **Do not port these semantics to Windows** — Windows
   uses Ctrl+Space chord (see `feedback_hotkey_per_platform.md`).
5. `apps/macos/App/ContentView.swift` — main window layout.
6. `apps/macos/App/LevelMeter.swift` — dB-normalized level meter math;
   reuse the formula verbatim.

`docs/design/identity-tokens.md` is the authoritative visual spec. Tailwind
config + shadcn theme on the Tauri side are the consuming layer — populate
them from the spec, don't hardcode.

## 4. Deliverables (MVP)

Minimum set for the Tauri app to replace the WinUI 3 Windows shell. Mirrors
what the Mac app already does; don't invent new features.

1. **Main window** — header, model-load progress, Record button,
   live level meter, transcript area, status line. Matches
   `ContentView.swift` functionally.
2. **Floating pill HUD** — second Tauri window, borderless, topmost,
   click-through while recording/transcribing, clickable while idle.
   Three states: idle dots → orange 12-bar level meter → transcribing
   spinner. Positioned bottom-center above taskbar/Dock.
3. **Tray / menubar icon** — mono idle, orange when recording; tooltip
   reflects phase; double-click opens main window; right-click context
   menu with Open / phase-aware dictation item / Quit.
4. **Global hotkey** — Windows: Ctrl+Space chord via the OS API (Tauri
   has `tauri-plugin-global-shortcut`). Mac (if Tauri ships on Mac):
   Right Cmd tap-not-hold will need a custom CGEventTap-based
   implementation since the plugin does chords only.
5. **Dictation flow** — capture via `core::audio`, transcribe via the
   platform-appropriate recognizer, post-process via `core::transcript`,
   deliver via paste.
6. **Text injection** — Ctrl+V on Windows / Cmd+V on Mac with clipboard
   preservation. Current Mac impl: `apps/macos/App/TextInjector.swift`.
   Windows impl: `apps/windows/OpenWhisper/TextInjection/TextInjector.cs`.
   Both are reference-quality; port the logic to Tauri's Rust side.
7. **Escape-to-cancel** during recording. Phase-gated in Rust core
   (`Core.RequestCancel`) — shell just forwards the keypress.
8. **Fullscreen-aware behavior** — pill hides when a fullscreen app is
   foreground; hotkey unregisters so the fullscreen app receives the
   combo normally. Re-activate on exit.
9. **Close-to-tray** — main window hides on close, app stays alive,
   only Quit from tray menu exits.
10. **Single-instance enforcement** — second launch focuses existing
    instance.

Features explicitly NOT in MVP (same as Mac today): settings UI, hotkey
rebinding UI, model picker, transcript history, vocabulary UI, mic
selection, onboarding.

## 5. Implementation notes

### Linking the Rust core

Add `core/` to the Tauri app's Cargo.toml as a path dependency:

```toml
[dependencies]
openwhisper-core = { path = "../../core" }
```

The C-ABI FFI layer (`core/src/ffi_c.rs`) stays — Mac still uses it. Tauri
skips it and calls the core's internal Rust API directly. Expose Tauri
commands (`#[tauri::command]`) that wrap core methods. For polled state
(phase, level), stream to the frontend via `window.emit` or a `tauri::State`.

Poll cadence from the frontend: 20 Hz (50 ms) matches Mac and Windows
shells — the level meter redraw and elapsed-time display rely on it.

### Pill as a separate Tauri window

Tauri supports multi-window apps. Configure the pill as a second window:

- `decorations: false`, `always_on_top: true`, `skip_taskbar: true`
- `transparent: true` on Windows + Mac; enable platform-specific blur via
  `window-vibrancy` crate or equivalent
- For click-through: `set_ignore_cursor_events(true)` while
  recording/transcribing; back to false on idle
- For capsule shape: border-radius on the outermost CSS element, and make
  the window transparent — no more OS-level `SetWindowRgn` gymnastics
  because the WebView handles alpha natively

This is where Tauri should visibly beat WinUI 3 — rounded / translucent
overlays are a solved problem in CSS + WebView.

### Hotkey

Windows: `tauri-plugin-global-shortcut` registers Ctrl+Space. Mac: the
plugin handles chords but not tap-not-hold semantics, so write a small
Rust module using `core-graphics` crate to install a CGEventTap and
replicate `HotkeyService.swift`. Escape-to-cancel: minimal additional
hook (Windows: `WH_KEYBOARD_LL` — see current
`apps/windows/OpenWhisper/Hotkey/EscapeHook.cs` for reference logic).

### Text injection

Cross-platform clipboard crates (`arboard`, `clipboard`) + platform-specific
key-event synthesis. Mirror the clipboard-save → paste → clipboard-restore
dance from `TextInjector.swift` / `TextInjector.cs`. 200ms restore delay
matches Mac.

### Tray icon

`tauri-plugin-tray-icon` (or Tauri 2's built-in `tray` module). Pre-render
two PNGs (idle mono, recording orange) or port the procedural mic-glyph
renderer from `apps/windows/OpenWhisper/Tray/StatusIconRenderer.cs` to Rust.

### Styling

shadcn/ui components are copied into the repo (`components/ui/`) rather
than imported — customize freely. Tailwind config sources tokens from
`docs/design/identity-tokens.md`:

- `recording` color → Tailwind custom color `recording: '#E07000'`
- Pill dimensions → CSS custom properties or Tailwind theme values
- Typography → Tailwind font-family + font-size scale
- Corner radii → Tailwind borderRadius theme

Font: system default (San Francisco on Mac, Segoe UI Variable on Win 11).
No bundled custom font.

## 6. Proposed directory layout

```
apps/
  macos/             (unchanged — reference)
  windows/           (unchanged — historical WinUI 3, kept for reference)
  tauri/             (NEW)
    src-tauri/       (Rust — Tauri backend)
      Cargo.toml     (references ../../core as a path dep)
      src/
        main.rs
        commands.rs  (tauri::command wrappers around core)
        hotkey.rs    (platform-specific hotkey)
        injection.rs (platform-specific paste)
        tray.rs      (tray icon + menu)
      tauri.conf.json
    src/             (React + TypeScript)
      main.tsx
      App.tsx        (main window)
      PillOverlay.tsx (pill window)
      components/ui/ (shadcn components)
      lib/
        tauri.ts     (typed wrappers around tauri::command invocations)
        tokens.ts    (visual tokens — sourced from docs/design)
    package.json
    tailwind.config.ts
    tsconfig.json
```

Confirm the directory name (`tauri/` vs `desktop/` vs something else) with
the user before scaffolding. `tauri/` is clear; `desktop/` leaves room if
the app ever ships mobile.

## 7. Phased plan

**Phase 0 — scaffold** (~half a day)
- `apps/tauri/` via `pnpm create tauri-app` (Tauri 2, React + TS template).
- Add Tailwind + shadcn-ui. Install base components (Button, InfoBar
  analog, Dialog, etc.).
- Wire `openwhisper-core` as a Cargo path dep in `src-tauri/Cargo.toml`.
- Smoke test: main window shows, `core::version()` returned via
  `tauri::command` and rendered.

**Phase 1 — main window parity** (~2–3 days)
- Port `ContentView.swift`'s layout to `App.tsx` using shadcn components.
- Expose `DictationService` state through Tauri events → React state
  (via `@tauri-apps/api` `listen`). 20 Hz tick.
- Model-load InfoBar, Record button, level meter ProgressBar, transcript
  TextBox analog, status line.

**Phase 2 — pill HUD** (~2 days)
- Second Tauri window, configured as listed in §5.
- React component for the three pill states, driven by the same event
  stream.
- CSS `backdrop-filter` for the blur/material. Handle reduced-motion +
  RDP-no-blur gracefully.
- Fullscreen detection on the Rust side (reuse the
  `GetForegroundWindow` + `rcMonitor` logic from `PillWindow.xaml.cs`).

**Phase 3 — tray + menu + hotkey** (~1–2 days)
- Tray icon with phase-aware swap + tooltip.
- Context menu mirroring the Mac menubar menu wording.
- Windows hotkey via `tauri-plugin-global-shortcut`; test Ctrl+Space chord.
- Escape-to-cancel via minimal low-level hook (port
  `EscapeHook.cs` logic).

**Phase 4 — dictation flow + text injection** (~1–2 days)
- Bridge Rust core's audio capture → transcription → delivery.
- Clipboard-save + paste + clipboard-restore dance.
- Fullscreen-aware hotkey disable + re-enable.

**Phase 5 — health banner, close-to-tray, single-instance** (~1 day)
- Banner shown when hotkey registration fails, with Retry.
- `tauri-plugin-single-instance` for second-launch focusing.
- Close → hide; only tray-menu Quit exits.

**Phase 6 — polish pass vs Mac reference** (~2 days)
- Side-by-side compare with Mac. Close remaining visual/behavioral gaps.
- Verify fullscreen behavior across Chromium fullscreen, video apps,
  games.
- RDP + multi-monitor sanity check.

**Ship criteria:** Tauri Windows build at feature + visual parity with
current Mac SwiftUI. At that point, discuss with user whether to also
migrate Mac to the Tauri build (replacing `apps/macos/`) or keep SwiftUI
shipped alongside.

## 8. Constraints and traps to know about

Pulled from user memories and prior sessions — read these before assuming:

- **Windows no-admin:** dev machine is a standard-user account. Per-user
  installers only; no VS Build Tools. Use Rust GNU toolchain. Tauri's
  MSVC toolchain is *probably* fine (VS Build Tools community edition
  has per-user flow) — confirm with user.
- **Windows dev box is RDP:** some visual material (Acrylic/Mica) is
  disabled on RDP. Test material effects on a local session before
  judging.
- **Windows path marshaling trap:** the username has a `ø`; native libs
  that marshal paths as LPStr/UTF-8 crash. The Rust side owns all path
  handling, so this is mostly a concern when passing paths through
  non-Rust libraries — `GetShortPathNameW` is the escape hatch.
- **Hotkey differs per platform:** Windows = Ctrl+Space chord, Mac =
  Right Cmd tap-not-hold. Do not unify.
- **Toolchain:** `fnm` for Node version, `pnpm` for all JS installs
  (including global). Never raw `npm`. Git Bash has `dotnet` on PATH via
  `.bashrc`; PowerShell does not.
- **Build Rust core in release during dev:** Debug core is 50–120×
  slower on DSP paths. Tauri's dev mode typically builds the Rust side
  in debug — investigate whether to override, mirroring `scripts/
  dev-run.sh`'s pattern.
- **Backlog:** tasks tracked via the Backlog.md CLI (`backlog` command).
  Tasks in `backlog/tasks/`, decisions in `backlog/decisions/`. Don't
  suggest GitHub Issues / Linear / TODO.md.
- **Orchestration lives in Rust core, not shell.** State machines, phase
  transitions, status strings, gating logic — Rust side. Shell polls
  a snapshot at 20 Hz. Resist the temptation to redo any of it in
  JavaScript.
- **Monetization:** local dictation is free forever. Reject feature
  proposals that gate core local features behind payment.
- **Zero-config over toggles:** lead with auto-detect, use settings only
  as fallback.
- **Local-first for cost features:** don't propose cloud LLM to save
  tokens on another cloud LLM. Rules-first, small local LLM if needed.

## 9. First steps for the next session

1. Read this file top to bottom, then skim the referenced Mac source files.
2. Read `docs/design/identity-tokens.md` and `docs/claude-windows-handoff.md`.
3. Confirm these with the user before writing any code:
   - Directory name: `apps/tauri/`? `apps/desktop/`?
   - Tauri 2 (latest) — confirm ok.
   - shadcn/ui over alternatives (Radix + Tailwind + shadcn, not MUI /
     Chakra / etc.) — confirm ok.
   - Should Mac migrate to Tauri once parity is reached, or run parallel
     indefinitely?
4. Propose Phase 0 scaffolding as a concrete, small PR. Don't bundle
   phases — land them incrementally.
5. Create backlog tasks for phases 0–6 via the Backlog.md CLI.
6. Mark `apps/windows/` as deprecated in a README note (don't delete yet).

## 10. State of the repo at pivot time

- `windows-port` branch: WinUI 3 Pass 1 committed (`9962e6a`) — pill HUD,
  tray, tray-only mode, tray menu, identity tokens, docs/design/
  identity-tokens.md.
- `windows-port-pass2` branch: Pass 2 committed (`12d01bd`) on top —
  Escape-to-cancel, health banner, fullscreen-aware pill + hotkey
  gating, larger pill dimensions.
- `main` branch: pre-Windows-port (Mac app + spike only).
- Memory system at `~/.claude/projects/C--Users-JimmiJ-nsson-Repositories-OpenWhisper/memory/`
  has been updated to reflect the pivot. Read `MEMORY.md` first.

Don't delete the WinUI 3 code. It's functional reference for every
platform-specific integration (hotkey, tray, fullscreen detection, paste
clipboard dance, settings JSON) and saves a lot of time when the Tauri
port needs the equivalent on Windows.
