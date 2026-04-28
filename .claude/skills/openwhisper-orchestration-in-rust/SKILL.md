---
name: openwhisper-orchestration-in-rust
description: Architecture rule — state machines, phase transitions, gating logic, and status strings live in the Rust core, not the platform shell. READ before proposing where to put logic that touches the dictation state machine, phase transitions, gating rules, or anything the UI reacts to. The temptation on every new shell is to keep "just the UI-coupled bits" in the shell — resist.
---

# Orchestration belongs in Rust core, not the shell

State machines, phase transitions, status strings, and gating logic (canToggle during transcribing, cancel-only-while-recording, "preparing…" vs "loading Parakeet…") all live in `core/`. Shells poll a snapshot (~20 Hz with `Mutex<State>`) and push events back. Phase values are exposed as `u32`-encoded enums — simpler than native enum FFI.

## What stays in core
- Pure computation with no OS deps (DSP, regex post-processing, transcript processing).
- State machine + flow coordination driving UI — the snapshot type, event-style entry points, all transitions and gating.
- Anything user-visible that's "the same decision on every platform" (status strings, empty-sample handling, cancel timing, i18n).

## What stays in the shell
- Actual OS APIs: hotkey registration (`tauri-plugin-global-shortcut` on Windows, `CGEventTap` on Mac), text injection, clipboard, media session, tray/menubar.
- UI widgets (React components, SwiftUI views).
- Thin observable glue mirroring the Rust snapshot — `useEffect` polling on the Tauri side, `@Observable` on macOS.

## Why

Solo dev + multi-shell = inevitable drift. If orchestration lives per shell, every semantic decision (status strings, empty-sample handling, cancel timing, i18n) gets re-implemented and diverges across Tauri/Swift/whatever-comes-next. The dictation state machine moved into Rust in early April 2026 (commit 5b30e02) for exactly this reason.

Even though Tauri's shell is also Rust, the same rule still applies inside the Tauri app: orchestration goes in `core/` (the cargo crate shared with the macOS SwiftUI shell), not in `src-tauri/` or `src/`. The Tauri Rust side is "shell-Rust" — handlers, plugin glue, OS adapters — not orchestration.

## How to apply

When proposing a feature, ask **"does this decision exist on macOS already?"** — if yes, the rule, status string, or transition belongs in `core/`, not in the shell. Resist the temptation to keep "just the UI-coupled bits" in the shell language. Accept the FFI / boundary overhead; don't argue "it's only 50 lines, duplicating is fine" — status strings, gating rules, error transitions, and edge cases (empty sample drain, cancel timing) accrete nuance.

When you find logic that's drifted into a shell (an `if phase === 'recording'` check in React, an `INotifyPropertyChanged` setter that decides UI text), the right fix is usually to push it down into the snapshot type and have the shell read it, not extend the shell logic.
