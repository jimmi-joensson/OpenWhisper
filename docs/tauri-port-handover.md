# Tauri port — handover prompt

Paste this (or the relevant sections) to a fresh Claude session or hand it to
a collaborator. Self-contained: a reader cold to the project should be able
to start from here.

---

## 1. What OpenWhisper is

OpenWhisper is an open-source local-dictation app — Superwhisper-class
(press hotkey, talk, tap again, transcript is pasted into the focused app).
Parakeet-TDT v3 (NVIDIA, CC-BY-4.0) runs on-device. MIT license. Local
dictation is free by default and must never be paywalled.

Shipped today (v0.2.0, first cross-platform release):

- **macOS**: SwiftUI/AppKit shell, FluidAudio (CoreML on ANE) via
  `apps/macos/`. Source of truth for behavior and visuals.
- **Windows**: WinUI 3 (C#) shell, sherpa-onnx ONNX via `apps/windows/`.
  Ships but visual quality doesn't match Mac. **Deprecated** — see §2.

Code layout today:

```
core/                   Rust crate — audio capture (cpal), phase machine,
                        transcript post-processing, config. C ABI + swift-bridge
                        exposed for shells. No recognizer yet (lives in shells).
apps/macos/             Swift + SwiftUI/AppKit shell. Source of truth.
apps/windows/           C# + WinUI 3 shell. DEPRECATED by this pivot. Kept
                        in git as reference until platform tricks are
                        archived into backlog decisions, then deleted.
apps/tauri/             (NEW — being created) Tauri 2 + React shell that
                        replaces both apps/macos/ and apps/windows/ once at
                        parity.
docs/                   tauri-port-handover.md (this file),
                        claude-windows-handoff.md (prior brief),
                        design/identity-tokens.md (visual spec)
backlog/                Backlog.md CLI task tracking
```

## 2. Why we're pivoting to Tauri

v0.2.0 shipped both platforms but the Windows WinUI 3 shell never matched
Mac visually (pill capsule material, overall feel). Cross-platform
consistency beats native-feel for this product — SuperWhisper's uniform
look on Mac and Windows is the target. Prior stance ("native UI per OS") is
reversed.

**Both macOS and Windows migrate to a single Tauri 2 app.** Mac SwiftUI
stays shipped until Tauri hits parity, then is replaced (not maintained
alongside). Windows WinUI 3 is deprecated immediately — no further
investment, no parallel ship after Tauri reaches parity.

Stack choices:

- **Tauri 2** — Rust backend + WebView frontend. Chosen over Electron:
  Rust-native fits our existing core, smaller binary, no Node runtime.
- **React + TypeScript**, **Tailwind CSS**, **shadcn/ui** (Radix-based,
  vendored into `components/ui/` — owned, not imported).
- **Existing Rust `core/`** linked as a Cargo path dep. No FFI crossing
  inside Tauri — it's already Rust.
- **Recognizer = sherpa-rs** (Rust bindings to sherpa-onnx) with CoreML EP
  on Mac and CPU / DirectML on Windows. Folded into `core/` (not left in
  the shell). Swift `@_cdecl` FluidAudio staticlib is the fallback if
  sherpa+CoreML regresses vs the shipped Mac baseline. See §6.

## 3. Current state of main (as of this doc)

`main` @ `03d394b`. v0.2.0 shipped. Branch has all Windows WinUI 3 work
merged (`12d01bd` pass 2, `33bd8dd` larger capsule, `34c75f9`+`03d394b`
publish-output fixes). The `windows-port` and `windows-port-pass2` branches
mentioned in earlier drafts of this file are merged into main.

No Tauri work yet — this is the brief for starting it.

## 4. Mac is the source of truth

When in doubt about behavior or visuals, read `apps/macos/App/` and mirror.
Files to study in order:

1. `apps/macos/App/OpenWhisperApp.swift` — app entry, tray icon, menu
   structure, lifecycle (370 lines).
2. `apps/macos/App/PillOverlay.swift` — signature visual (pill HUD states,
   positioning, click-through, grace-return) (215 lines).
3. `apps/macos/App/DictationService.swift` — orchestration surface the UI
   reacts to (276 lines).
4. `apps/macos/App/HotkeyService.swift` — Right Cmd tap-not-hold activation
   (238 lines). **Do not port this to Windows** — Windows uses Ctrl+Space
   chord (see `feedback_hotkey_per_platform.md`).
5. `apps/macos/App/ContentView.swift` — main window layout (175 lines).
6. `apps/macos/App/LevelMeter.swift` — dB-normalized level meter math (39
   lines); reuse the formula verbatim.
7. `apps/macos/App/TextInjector.swift` — clipboard-save → paste →
   clipboard-restore dance (79 lines).

`docs/design/identity-tokens.md` is the authoritative visual spec. Tauri
side generates `tokens.ts` + Tailwind theme **from** that spec — don't
hardcode values, don't let the two drift.

Do NOT mirror WinUI 3 visuals — it's the thing we're replacing.

## 5. Deliverables (MVP for Tauri)

Minimum set for the Tauri app to replace **both** shipped shells at
parity. Mirrors what the Mac app does today. No new features.

1. **Main window** — header, model-load progress, Record button, live
   level meter, transcript area, status line. Matches `ContentView.swift`
   functionally.
2. **Floating pill HUD** — second Tauri window, borderless, topmost,
   click-through while recording/transcribing, clickable while idle.
   Three states: idle dots → orange 12-bar level meter → transcribing
   spinner. Bottom-center above taskbar/Dock.
3. **Tray / menubar icon** — mono idle, orange when recording; tooltip
   reflects phase; double-click opens main window; right-click context
   menu (Open / phase-aware dictation item / Quit).
4. **Global hotkey** — Windows: Ctrl+Space chord via
   `tauri-plugin-global-shortcut`. Mac: Right Cmd tap-not-hold via a
   custom Rust module using `core-graphics` CGEventTap (plugin handles
   chords only).
5. **Dictation flow** — capture via `core::audio`, recognize via sherpa-rs
   in `core/` (new), post-process via `core::transcript`, deliver via
   paste.
6. **Text injection** — Ctrl+V on Windows / Cmd+V on Mac with clipboard
   preservation. Port logic from `apps/macos/App/TextInjector.swift` to
   Rust using `arboard` + platform key-event synthesis (200 ms restore
   delay matches Mac).
7. **Escape-to-cancel** during recording. Phase-gated in Rust core
   (`Core.RequestCancel`). Minimal low-level key hook per platform —
   reference `apps/windows/OpenWhisper/Hotkey/EscapeHook.cs` for Windows
   logic.
8. **Fullscreen-aware behavior** — pill hides when a fullscreen app is
   foreground; hotkey unregisters so the fullscreen app receives the
   combo normally. Re-activate on exit. Port Windows detection from
   `PillWindow.xaml.cs`; Mac detection from `PillOverlay.swift`.
9. **Close-to-tray** — main window hides on close, app stays alive,
   only Quit from tray menu exits.
10. **Single-instance enforcement** via `tauri-plugin-single-instance`.
11. **Health banner** when hotkey registration fails, with Retry.
12. **Auto-update** via `tauri-plugin-updater` (WinUI 3 had none — cheap
    to wire now).

Features explicitly NOT in MVP (same as Mac today): settings UI, hotkey
rebinding UI, model picker, transcript history, vocabulary UI, mic
selection, onboarding.

## 6. Recognizer decision

`core/` today has no recognizer — it's in the shells (Swift FluidAudio on
Mac, C# sherpa-onnx on Windows). For Tauri, recognizer moves into
`core/` so the shell only polls phase + transcript.

**Primary path — sherpa-rs + CoreML EP.** Same library Windows shell
already proved (`c0c1d2e`), with CoreML execution provider on Mac for ANE
access. Single codepath across OSes. Keeps Parakeet v3 + its EN/DA
behavior already characterized in memory.

**Fallback — Swift `@_cdecl` FluidAudio staticlib.** If the sherpa+CoreML
spike regresses vs the shipped FluidAudio baseline (latency, WER, or ANE
utilization), scaffold a Swift package exposing C fns wrapping FluidAudio,
link into Rust via `build.rs` shelling `swiftc`. XPC sidecar is the
tertiary option, not a default.

**Do NOT** drive CoreML directly from Rust via `objc2-core-ml`. That's the
"hand-roll conversion" path `project_stt_engine.md` memory explicitly
rules out.

The recognizer spike is early in the phase order (§9) — it's the biggest
unknown and gates the port.

## 7. Implementation notes

### Linking the Rust core

Add `core/` to the Tauri app's Cargo.toml as a path dependency:

```toml
[dependencies]
openwhisper-core = { path = "../../core", default-features = false, features = ["tauri"] }
```

**Before Phase 0:** feature-gate `swift-bridge` and `swift-bridge-build` in
`core/Cargo.toml` so the Tauri target doesn't compile Swift FFI bindings
unnecessarily. Add a `macos-shell` feature for the existing swift-bridge
codepath; default features stay backward-compatible for the shipped Mac
SwiftUI app.

The C-ABI FFI layer (`core/src/ffi_c.rs`) stays — Mac SwiftUI still uses
it. Tauri skips it and calls the core's internal Rust API directly via
`#[tauri::command]` wrappers.

Frontend poll cadence: 20 Hz (50 ms) matches the Mac level meter and
elapsed-time display math. Can be implemented as Tauri events emitted
from Rust on a timer, or as React `invoke()` polling — pick one.

### Rust core release build during Tauri dev

Debug Rust is 50–120× slower for the rubato sinc resample path (see
`feedback_rust_release_in_dev_loop.md`). Tauri dev mode builds the Rust
backend debug by default. Mirror `scripts/dev-run.sh` pattern: build
`openwhisper-core` release, Tauri shell debug. Investigate
`[profile.dev.package.openwhisper-core] opt-level = 3` in the workspace
Cargo.toml as the simplest override.

### Pill as a separate Tauri window

- `decorations: false`, `alwaysOnTop: true`, `skipTaskbar: true`,
  `transparent: true`
- Click-through: `set_ignore_cursor_events(true)` while recording /
  transcribing; back to false on idle
- Capsule shape: CSS `border-radius` on the outermost element; window
  transparent — no OS-level `SetWindowRgn` gymnastics
- Material: CSS `backdrop-filter: blur()`. Handle reduced-motion +
  RDP-no-blur gracefully (flat fallback).
- Platform blur (macOS vibrancy, Windows Mica) via the `window-vibrancy`
  crate if CSS blur insufficient.

Pill is the piece that failed on WinUI 3. Build it as a **spike in Phase
1** (before main window) to burn down visual risk early. If CSS + WebView
can't deliver rounded + translucent + click-through + always-on-top on
Windows RDP, the whole port is at risk — find out cheaply.

### Hotkey

Windows: `tauri-plugin-global-shortcut` registers Ctrl+Space. Mac: plugin
handles chords but not tap-not-hold semantics, so write a small Rust
module using the `core-graphics` crate to install a CGEventTap and
replicate `HotkeyService.swift`. Escape-to-cancel: minimal low-level
hook per platform.

### Text injection

Cross-platform: `arboard` for clipboard, platform-specific key-event
synthesis (evaluate `enigo` vs direct platform APIs). Mirror the
clipboard-save → paste → clipboard-restore dance from
`TextInjector.swift` / `TextInjector.cs`. 200 ms restore delay matches
Mac.

### Tray icon

Tauri 2's built-in tray module. Pre-render two PNGs (idle mono, recording
orange) or port the procedural mic-glyph renderer from
`apps/windows/OpenWhisper/Tray/StatusIconRenderer.cs` to Rust. Archive
the C# version to a backlog decision before deleting.

### Styling + tokens

shadcn/ui components vendored into `apps/tauri/src/components/ui/`.
**Tokens are script-generated** from `docs/design/identity-tokens.md`:

- A build script emits `apps/tauri/src/lib/tokens.ts` (TS constants) and
  Tailwind theme values read from the same source
- `recording: '#E07000'`, pill dimensions, typography scale, corner radii
  all live in identity-tokens.md and flow through the generator
- Do NOT copy values manually — drift is guaranteed

Font: system default (San Francisco on Mac, Segoe UI Variable on Win 11).
No bundled custom font.

### macOS Tauri packaging — open work

- Entitlements: mic, Accessibility (for key synthesis), Input Monitoring
  (for CGEventTap hotkey)
- Codesigning: Developer ID if we want notarization; ad-hoc sign for dev
- TCC grants invalidate on ad-hoc re-sign — need a Tauri-side equivalent
  of `scripts/dev-run.sh`'s reset-tcc + re-launch cycle

These are Phase 4 / Phase 5 concerns; don't block Phase 0 on them.

## 8. Proposed directory layout

```
apps/
  macos/             (unchanged — shipped, replaced when Tauri hits parity)
  windows/           (historical — to be archived + deleted; platform
                     tricks extracted to backlog decisions first)
  tauri/             (NEW)
    src-tauri/       (Rust — Tauri backend)
      Cargo.toml     (references ../../core as a path dep with a "tauri" feature)
      src/
        main.rs
        commands.rs  (#[tauri::command] wrappers around core)
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
        tauri.ts     (typed wrappers around #[tauri::command] invocations)
        tokens.ts    (GENERATED from docs/design/identity-tokens.md)
    scripts/
      gen-tokens.ts  (reads identity-tokens.md, emits tokens.ts + tailwind bits)
    package.json
    tailwind.config.ts
    tsconfig.json
```

Directory name `apps/tauri/` confirmed (over `apps/desktop/`).

## 9. Phase plan (pill-first, recognizer-early)

Order diverges from a naive "main window first" sequence — highest-risk
pieces go first so failures are cheap.

**Phase 0 — scaffold + core wiring** (~1 day)
- `apps/tauri/` via `pnpm create tauri-app` (Tauri 2, React + TS).
- Tailwind + shadcn-ui base install.
- Feature-gate `swift-bridge` in `core/Cargo.toml`; Tauri target skips it.
- Wire `openwhisper-core` as a Cargo path dep.
- Workspace `[profile.dev.package.openwhisper-core] opt-level = 3`.
- Smoke test: main window shows, `core::version()` via `#[tauri::command]`.

**Phase 1 — pill HUD spike** (~2 days)
- Second Tauri window with decorations off, transparent, always-on-top,
  skip taskbar.
- CSS capsule + backdrop-filter + click-through toggle.
- Three states wired to hardcoded mock state (no core integration yet).
- Mac + Windows (**including RDP box**) visual verification vs
  `PillOverlay.swift` reference.
- **Gate:** if visual parity unachievable or click-through breaks under
  RDP, pause and reconsider approach.

**Phase 2 — recognizer spike** (~2–3 days)
- Fold `sherpa-rs` into `core/` behind a `recognizer` module.
- Load Parakeet v3 via sherpa + CoreML EP on Mac from Rust.
- Bench vs shipped FluidAudio on a fixed clip: latency, WER, ANE util.
- **Gate:** if regression exceeds acceptable bounds, scaffold Swift
  `@_cdecl` FluidAudio staticlib as fallback.

**Phase 3 — main window parity** (~2–3 days)
- Port `ContentView.swift` layout to `App.tsx` using shadcn components.
- 20 Hz state stream via Tauri events → React state.
- Model-load banner, Record button, level meter, transcript area, status
  line.

**Phase 4 — tray + hotkey + Escape hook** (~2 days)
- Tray icon with phase-aware swap + tooltip + context menu.
- Windows hotkey via `tauri-plugin-global-shortcut` (Ctrl+Space).
- Mac hotkey via `core-graphics` CGEventTap (Right Cmd tap-not-hold).
- Escape-to-cancel via minimal low-level hook per platform.

**Phase 5 — dictation flow + text injection** (~2 days)
- Wire recognizer → core → delivery end-to-end.
- Clipboard-save + paste + clipboard-restore dance.
- Fullscreen-aware hotkey disable + re-enable.

**Phase 6 — close-to-tray, single-instance, health banner, auto-update**
(~1–2 days)
- `tauri-plugin-single-instance` for second-launch focusing.
- Close → hide; only tray-menu Quit exits.
- Health banner + Retry when hotkey registration fails.
- `tauri-plugin-updater` wired with placeholder endpoint.

**Phase 7 — polish pass vs Mac reference** (~2 days)
- Side-by-side compare with `apps/macos/`. Close remaining gaps.
- Verify fullscreen behavior across Chromium fullscreen, video apps,
  games.
- RDP + multi-monitor sanity check.
- Archive `apps/windows/` platform tricks to backlog decisions; delete
  `apps/windows/`.

**Ship criteria:** Tauri build at feature + visual parity with shipped
Mac SwiftUI on both OSes. Mac SwiftUI replaced, `apps/macos/` retired.

Realistic total: **15–20 working days**.

## 10. Constraints and traps

Pulled from user memories and prior sessions — read these before assuming:

- **Windows no-admin:** dev machine is a standard-user account. Per-user
  installers only; no VS Build Tools. Use Rust GNU toolchain. Tauri's
  MSVC toolchain needs confirmation (VS Build Tools community edition
  has per-user flow).
- **Windows dev box is RDP:** Acrylic/Mica are disabled on RDP. Test
  material effects on a local session before judging. Pill spike (Phase
  1) must verify on RDP.
- **Windows path marshaling trap:** the username has a `ø`; native libs
  that marshal paths as LPStr/UTF-8 crash. Rust owns path handling.
  `GetShortPathNameW` is the escape hatch if non-Rust libs are involved.
- **Hotkey differs per platform:** Windows = Ctrl+Space chord, Mac =
  Right Cmd tap-not-hold. Do not unify.
- **Toolchain:** `fnm` for Node version, `pnpm` for all JS installs
  (including global). Never raw `npm`. Git Bash has `dotnet` on PATH via
  `.bashrc`; PowerShell does not.
- **Rust core release in dev loop:** Debug core is 50–120× slower on DSP
  paths. See §7 for the `opt-level` workaround.
- **TCC grants on Mac:** ad-hoc sig changes nuke Accessibility / Input
  Monitoring grants. Tauri dev cycle needs an equivalent of
  `scripts/reset-tcc.sh`.
- **Backlog:** tasks via `backlog` CLI in `backlog/tasks/`, decisions in
  `backlog/decisions/`. Don't propose GitHub Issues / TODO.md.
- **Orchestration in Rust core, not shell.** State machines, phase
  transitions, status strings, gating logic — Rust. Shell polls at 20 Hz.
- **Monetization:** local dictation is free forever. Reject proposals to
  gate local features behind payment.
- **Zero-config over toggles:** lead with auto-detect, settings only as
  fallback.
- **Local-first for cost features:** don't propose cloud LLM to save
  tokens on another cloud LLM.

## 11. First steps for the next session

1. Read this file top to bottom, then skim the Mac source files (§4).
2. Read `docs/design/identity-tokens.md` and
   `docs/claude-windows-handoff.md` (the latter for historical context
   on Windows decisions).
3. Confirm any deviations from §2 / §6 / §9 with the user.
4. Start Phase 0 as a small PR. Don't bundle phases. Each phase = its own
   merge.
5. Backlog tasks for phases 0–7 exist — check `backlog tasks` list.
6. Mark `apps/windows/` deprecated in a README note; don't delete until
   Phase 7.

Don't delete the WinUI 3 code prematurely. It's functional reference for
every platform-specific integration Windows Tauri will need (hotkey
chord registration, tray icon render, fullscreen detection, paste
clipboard dance, EscapeHook low-level hook, settings JSON). Archive to
backlog decisions with code inline during Phase 7, then delete.
