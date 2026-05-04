---
id: doc-24
title: Library API audit and headless CLI — design
type: spec
created_date: '2026-05-04 15:06'
---

# Library API audit + headless CLI — design

**Backlog parent:** TASK-81
**Milestone:** m-1 — v1.0 public release readiness

## Problem

Today `openwhisper-core` already exposes `pub mod audio`, `dictation`, `recognizer`, `transcript`, `verbose` and is consumed by the Tauri shell as a normal cargo dependency. So the library-first architecture is *technically* in place. But:

1. **The public API has not been treated as a public API.** No `core::prelude`, partial doc-comments, no `#[non_exhaustive]` discipline on enums/structs that will outlive v1, no consideration for ergonomics from outside the workspace.
2. **Orchestration has leaked into the Tauri shell.** `apps/tauri/src-tauri/src/lib.rs` is 1081 lines. Some of that is unavoidable platform glue (NSPanel conversion, tauri-nspanel, hotkey hooks, fullscreen detection, MediaController per-OS). But behaviors like the `pause_audio_for_recording` / `resume_audio_after_recording` lifecycle gate, the `PAUSED_BY_US` invariant, and the dictation-phase ↔ media-controller coupling are state-machine orchestration. Per the `openwhisper-orchestration-in-rust` rule, that belongs in core.
3. **No headless surface.** `core/examples/recognizer_smoke.rs` is the only library consumer outside Tauri. There's no CLI an external contributor can run to repro a transcription bug, no automated way for CI to smoke the engine without a windowed runner, no debug surface for Linux contributors who can't build the Tauri shell yet.

## Goal

By construction of the architecture, **every feature available in the GUI is available via CLI is available in the public library**.

Specifically:

- A new `cli/` workspace member that wraps the same `openwhisper-core` public API the Tauri shell consumes. No CLI-private logic in `core/`. No GUI-private logic in `apps/tauri`.
- The Tauri shell's Rust commands (`#[tauri::command]`) become one-liners that delegate to library calls. Platform glue (NSPanel, TCC, hotkey hook, tray) stays in the shell because it has no analog in a headless context.
- An external contributor can `cargo run -p openwhisper-cli -- transcribe sample.wav` and get a transcript on either Mac or Windows without launching Tauri or granting TCC.
- CI can smoke the recognizer pipeline by running the CLI against a bundled sample WAV — no virtual desktop, no Playwright, no permissions dance.

## Non-goals

- **Linux port.** The recognizer trait permits a Linux backend (ort + CPU), but wiring it is out of scope for v1 and doesn't block any of this work.
- **CLI feature surface beyond the v1 set.** `transcribe`, `enumerate-devices`, `recognizer-info`, `crash-dump` are the v1 subcommands. Hotkey simulation, settings export, model download — all defer to v1.x.
- **Renaming.** Stay on `openwhisper-core`, `openwhisper-cli`. The rename happens later as a final sweep before public flip (TASK-NEW-5).
- **Making the public API permanent.** v1.0 is the first time we'll expose `core` to external Rust consumers, but we are *not* committing to semver-stability at v1.0. The `#[non_exhaustive]` discipline is to avoid pinning ourselves *too* hard before the API has external pressure on it.

## Behavior model

The library, the CLI, and the Tauri shell share one mental model:

```
                  ┌─────────────────────────────┐
                  │    openwhisper-core (lib)    │
                  │                             │
                  │  prelude::*                 │
                  │  ├─ DictationEngine         │
                  │  │   (state machine)        │
                  │  ├─ AudioCapture            │
                  │  ├─ Recognizer trait        │
                  │  │   ├─ FluidAudio (mac)    │
                  │  │   └─ Sherpa     (win)    │
                  │  ├─ Transcript pipeline     │
                  │  └─ Diagnostics readout     │
                  └────────┬────────────┬───────┘
                           │            │
                  ┌────────▼─────┐  ┌──▼──────────────┐
                  │ openwhisper- │  │ apps/tauri/     │
                  │   cli (bin)  │  │ src-tauri (bin) │
                  │              │  │                 │
                  │ clap parser  │  │ Tauri commands  │
                  │   ↓          │  │   ↓             │
                  │ lib calls    │  │ lib calls       │
                  └──────────────┘  └─────────────────┘
                                         │
                                    ┌────▼────────────┐
                                    │ Platform glue   │
                                    │ - NSPanel       │
                                    │ - hotkey hook   │
                                    │ - tray          │
                                    │ - TCC reset     │
                                    │ - fullscreen    │
                                    └─────────────────┘
```

What lives where:

| Concern | Lives in |
|---|---|
| State machine: `Idle → Loading → Recording → Transcribing → Idle` | core (already does — `dictation::*`) |
| Audio capture (cpal) | core (already does — `audio::*`) |
| Recognizer trait + Mac/Win impls | core (already does — `recognizer::*` feature-gated) |
| Transcript filter pipeline | core (already does — `transcript::*`) |
| Settings schema + persistence | core (move from shell — currently split) |
| Media-controller pause/resume *gate* (when to pause, when to resume, idempotency invariants) | **core** (currently in shell) |
| Media-controller *implementation* (AppleScript / SMTC bindings) | shell (platform glue) |
| Diagnostics readout (recognizer info, paths, version, EPs available) | core |
| Crash-dump file format + read API | core (writer ships in TASK-78; reader can ship now in stub form) |
| NSPanel conversion, TCC reset, fullscreen detection, hotkey hook, tray | shell (no CLI analog) |
| Tauri `#[command]` wrappers | shell (each is one-liner over library) |
| clap subcommand wrappers | cli (each is one-liner over library) |

### CLI surface for v1

```
openwhisper-cli transcribe <wav>             # offline batch transcribe; prints text
openwhisper-cli enumerate-devices            # JSON list of input devices the engine sees
openwhisper-cli recognizer-info              # which engine, model path, EP, version
openwhisper-cli crash-dump [--latest|--id N] # markdown dump of crash file (stub until TASK-78)
openwhisper-cli --version                    # crate version
```

`--json` flag on every subcommand for machine-readable output (stdout = data, stderr = log).

### What the public API will look like (sketch)

```rust
// openwhisper-core
pub mod prelude {
    pub use crate::audio::{AudioCapture, AudioDevice, CaptureError};
    pub use crate::dictation::{DictationEngine, Phase, Snapshot, Toggle};
    pub use crate::recognizer::{Recognizer, RecognizerInfo, TranscribeError};
    pub use crate::transcript::{TranscriptFilter, TranscriptStage};
    pub use crate::diagnostics::{
        CrashDump, CrashDumpReader, CrashId, DiagnosticsReadout, default_crash_reader,
    };
    pub use crate::settings::{Settings, SettingsStore};
}
```

The `prelude::*` import gives a one-line entry point for the CLI and for Tauri commands. Existing modules stay; this is reorganization + ergonomics, not a rewrite.

### Crash-dump contract (split between TASK-81 and TASK-78)

`core::diagnostics` lands the *trait surface* in TASK-81 (Task 2 Commit D). TASK-78 lands the *concrete implementation*:

```rust
// landed in TASK-81.2 (Task 2 Commit D)
#[non_exhaustive]
pub struct CrashId(/* opaque, displayed as string */);

#[non_exhaustive]
pub struct CrashDump {
    // Concrete fields filled by TASK-78. Until then, an empty
    // #[non_exhaustive] struct is enough to compile against.
}

#[derive(Debug)]
pub enum ReadError { NotFound, Io(std::io::Error), /* TASK-78 may add */ }

pub trait CrashDumpReader {
    fn list(&self) -> Vec<CrashId>;
    fn read(&self, id: &CrashId) -> Result<CrashDump, ReadError>;
}

// In TASK-81: returns None (no concrete reader exists yet).
// In TASK-78: returns Some(FileBackedCrashDumpReader::new(crash_dir)).
pub fn default_crash_reader() -> Option<Box<dyn CrashDumpReader>>;
```

TASK-81's CLI subcommand (`cli crash-dump`) calls `default_crash_reader()`; if `None`, it prints the deferred-feature notice and exits 0. TASK-78 swaps the implementation in without touching the CLI handler.

## Trade-offs

| Choice | Alternative | Why this |
|---|---|---|
| New top-level `cli/` crate | `[[bin]]` inside `core/` | Cleaner separation: `core/` stays a pure library, can ship as a published crate later without the CLI dragging clap into the dep tree. Tailscale's `cmd/tailscale` ↔ `cmd/tailscaled` split is the reference. |
| `clap` for arg parsing | hand-rolled, `argh`, `pico-args` | clap is the de-facto Rust standard, derive macros keep the CLI declaration close to subcommand handlers, `--help` and shell completion come free. Cost: ~200KB binary growth. Worth it. |
| Full audit + extraction in one milestone | Defer extraction; just add CLI on top of current API | Defer would mean the CLI ends up reaching into private internals or duplicating logic, which is exactly the drift this work is supposed to prevent. |
| `#[non_exhaustive]` on public enums/structs | Lock the API at v1.0 | We have <100 hours of external-API exposure across all of OpenWhisper's lifetime so far. Locking now would be premature. `#[non_exhaustive]` lets us add variants in v1.x without it being a breaking change. |
| Crash-dump CLI subcommand stubbed before TASK-78 lands | Wait for TASK-78 | Stubbing now means the CLI surface is set, the subcommand registers, and TASK-78 just fills in the file-read logic. Smaller, safer change. |
| CI smoke runs the CLI directly | CI smoke runs a separate test binary | The CLI *is* the smoke test. If it works, the engine works. One fewer surface to maintain. |

## Risk register

- **Hidden coupling between Tauri commands and shell-only state.** The 1081-line `lib.rs` likely has shell-only invariants the audit will surface. Mitigation: Task 1 is explicitly a non-destructive audit before Task 2 starts moving code.
- **macOS Playwright suite breakage during shell refactor.** Task 10 refactors Tauri commands to one-liners — risks regressing the Pill HUD or Settings panes. Mitigation: `pnpm test:ui` is explicit in Task 10's verification.
- **Windows recognizer divergence.** Mac uses FluidAudio (Swift bridge); Windows uses ort + sherpa-onnx. The CLI must work on both. Mitigation: CI smoke (Task 9) runs on the Windows runner too.
- **Iteration budget on the audit.** Per `openwhisper-iteration-budget`, two attempts max before research. The audit (Task 1) explicitly *is* the research step — Task 2 onward executes on its findings.
