---
name: project-principles
description: OpenWhisper's product values and cross-platform UX rules. READ before proposing features, settings, paywalls, activation/hotkey changes, or anything that affects how the app feels to a user. Pulls together the constraints that make OpenWhisper "the open alternative to Superwhisper" rather than just-another-dictation-app.
---

# OpenWhisper project principles

These are durable values, not implementation details. If a proposed change conflicts with one of them, surface that explicitly to the user — don't silently rationalize the conflict away. The canonical longer-form context lives in `docs/tauri-port-handover.md` and `docs/claude-windows-handoff.md`; this file is the short, applied form.

---

## Monetization — local dictation must never be paywalled

Paid tiers are only justified for features that cost the project to run (hosted sync, managed billing, server-side infra). All cloud integrations are BYO API key — never a markup-resale.

**Why:** OpenWhisper's positioning is "open alternative to Superwhisper." Superwhisper paywalls good local models — that gap is the wedge. Violating this erases the reason the project exists.

**How to apply:** Reject design proposals that gate any local feature behind payment. Gray-area cases (e.g. "premium voice activity detection") get flagged to the user — values calls are theirs, not yours.

---

## Toggle activation — not press-and-hold, not wake-word

Activation is tap-to-start / tap-to-stop. First hotkey press starts recording + shows the pill HUD; second press stops, transcribes, injects the text into the focused field. The hotkey must be fully rebindable, including single modifier keys and double-tap chords.

**Why:** Matches Superwhisper's muscle memory. Press-and-hold breaks long dictations (finger fatigue, accidental release). Wake-word adds always-on listening privacy concerns. Continuous-mode conflicts with the "dictation burst" workflow.

**How to apply:** Don't propose press-and-hold, wake-word, or continuous dictation unless the user explicitly reopens the question. Even when a platform-native idiom suggests otherwise (e.g. Windows Win+H), still default to toggle — cross-platform consistency outranks platform convention here.

---

## Hotkey semantics differ per platform — don't unify them

- **Windows default:** Ctrl + Space chord, registered via `tauri-plugin-global-shortcut` (Win32 `RegisterHotKey` under the hood).
- **macOS default:** Right Command tap-not-hold (press-alone, release-without-intervening-key → toggle), via a custom `CGEventTap` Rust module.

**Why:** Platform conventions differ. Chord-based hotkeys feel native on Windows and `RegisterHotKey` is the low-friction path (no LL keyboard hook, no AV flagging, no elevated trust). Tap-not-hold on a single modifier feels native on Mac (single-hand activation pairs with Mac's keyboard fluency; Right Cmd is the least-used modifier so claiming it is low-impact).

**How to apply:**
- Treat "cross-platform visual identity" and "cross-platform hotkey semantics" as orthogonal — visual identity is shared, activation is platform-idiomatic.
- Don't propose porting Mac's tap-not-hold semantics to Windows. Any Windows global-keyboard work (e.g. Escape-to-cancel) needs its own justification — it can't ride a hook that isn't installed.
- Both platforms share the same in-app rebind UI but persist platform-appropriate defaults.

---

## Zero-config over toggles — lead with auto-detect

Default to auto-detect / seamless behavior. Only add a setting when auto-detection genuinely can't disambiguate, or as a power-user escape hatch. Lead feature proposals with what the app *does automatically*; only after that describe the manual override, and only if there's a real case the auto path can't handle.

**Why:** The wedge is "strong local transcription, no configuration theatre." Toggle-heavy dictation apps feel like tax software. Settings are cheap to add and expensive to maintain (i18n, test matrix, support load, decision fatigue).

**How to apply:** When proposing any new UX, the first sentence should describe automatic behavior. If you can't state the auto path, the feature isn't ready to propose.

---

## Local-first for cost-saving features — never cloud-to-save-cloud

Any feature whose value proposition is "reduce token cost for the user" must be local-only. Using a cloud LLM to pre-compress / filter / clean text before sending to another cloud LLM is an economic inversion — don't build it. Rules first; small local LLM if rules aren't enough. Cloud is acceptable only for *capability* features the user explicitly opts into (alternative STT backends, premium translation), never for cost optimization.

**Why:** Values call, not a technical limitation. OpenWhisper's positioning is local-first-because-it's-free. Using cloud to save cloud cost contradicts the positioning and rarely math-checks out anyway.

**How to apply:** When proposing any "cleanup pass," "compression filter," or "preprocessing step" (caveman mode, transcript repair, etc.), default to rule-based or local-LLM. If the proposal uses cloud, the justification must be capability (something rules/local can't do), not cost.

---

## Cross-platform visual identity — Mac is the source of truth

OpenWhisper must be visually and behaviorally recognizable as the *same product* across macOS, Windows, and (future) Linux. Shared design vocabulary: recording orange `#E07000`, HUD pill with level-meter states (idle dots → recording bars → transcribing spinner), tray/menubar icon state changes, auto-paste semantics, toggle UX.

**Implementation strategy (since 2026-04-24):** single Tauri 2 shell with React + TypeScript + Tailwind + shadcn/ui, backed by the existing Rust `core/` (linked as a Cargo path dep, no FFI inside Tauri). The earlier "native UI per OS" stance was reversed after the WinUI 3 Windows port produced a visibly different pill / material / feel vs. Mac SwiftUI.

**Mac is the source of truth** for behavior and visual spec. Tauri mirrors Mac. Mac itself may stay SwiftUI indefinitely or migrate to Tauri later. Pixel-perfect parity isn't the goal — "close + recognizable" is.

**How to apply:**
- Visual-consistency-across-platforms outranks native-feel-per-OS. Pick the cross-platform rendering over a platform-idiomatic replacement.
- Shared design tokens live in `docs/design/identity-tokens.md`. Tauri's Tailwind config + shadcn theme are the consuming layer — don't hardcode values, don't let them drift.
- When in doubt about behavior or visuals, read `apps/macos/App/` and mirror — `PillOverlay.swift`, `DictationService.swift`, `ContentView.swift`, `LevelMeter.swift`, `TextInjector.swift` are the load-bearing files.
- Don't propose porting to yet another shell (WinUI, GTK, Qt). The cross-platform shell is Tauri.
- Platform-specific affordances that genuinely matter (hotkey semantics, OS-level text injection, tray vs menubar) are the only places the code branches by platform.
