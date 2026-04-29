# OpenWhisper

Open-source, local-first dictation for macOS. An alternative to Superwhisper where **local models are free** and you bring your own keys for any cloud providers you want to use.

## Why

Existing dictation tools paywall good local transcription. Their free local models mis-spell constantly and force you to hand-curate vocabulary. OpenWhisper runs strong local models (Whisper family, Parakeet, etc.) out of the box at no cost — you only pay if you opt into cloud features with your own API keys, or if you subscribe to future hosted conveniences (e.g. synced settings, managed billing). Core dictation stays free.

## Status

Very early. Single Tauri app shell targeting **macOS** + **Windows**, using NVIDIA Parakeet (CC-BY-4.0). Mac path runs FluidAudio/CoreML on the Apple Neural Engine; Windows path runs sherpa-onnx on CPU. Linux port to follow. The retired SwiftUI shell lives at `archive/macos/` for reference.

## How it works (MVP)

1. Press the activation hotkey — OpenWhisper starts listening and a pill overlay appears.
2. Talk.
3. Press the same hotkey again — recording stops, transcription runs locally on the ANE, and the resulting text is pasted into the currently focused input field.

Hotkey defaults to match Superwhisper's for familiarity; fully rebindable in settings.

## Stack

- **Shared core:** Rust (audio capture, VAD, dictation phase machine, transcript post-processing, BYO-key cloud providers)
- **App shell:** Tauri 2 (single codebase, Mac + Windows). React/TypeScript frontend, Rust backend, WebView UI.
- **Recognizer (Mac):** FluidAudio + Parakeet CoreML on Apple Neural Engine
- **Recognizer (Windows):** sherpa-onnx + Parakeet ONNX on CPU
- **Linux** (future): same Tauri shell, ONNX Runtime (CUDA/ROCm/CPU)

## Principles

- **Local-first, free by default.** Strong local transcription with no paywall.
- **BYO keys.** Any cloud model integration uses the user's own API credentials.
- **No dark patterns.** Transparent pricing, opt-in upgrades, no manipulative trial expirations or hidden upsells.
- **Correctable.** Custom vocabulary, post-processing, and prompt shaping are first-class — not afterthoughts.

## Install (pre-built)

No pre-built binaries yet — the Tauri release pipeline is being rebuilt (see TASK-46). For now, build from source.

## Building from source

**Prerequisites:** Rust (install via [rustup](https://rustup.rs/)), Node.js 20+, and [pnpm](https://pnpm.io/installation). Mac builds also need Xcode command-line tools (for AppKit linking).

```sh
cd apps/tauri
pnpm install
pnpm dev:tauri          # full bundled dev cycle (recommended on macOS — see apps/tauri/scripts/dev-run.sh)
# or:
pnpm tauri dev          # bare cargo run, no .app bundle (no TCC grants)
```

The Rust core (`core/`) builds as a normal cargo dependency of `apps/tauri/src-tauri`. No Xcode project, no swift-bridge for the Tauri shell.

### Packaging a release locally

```sh
cd apps/tauri
pnpm tauri build        # produces a platform-native bundle (.app + .dmg on Mac, .msi on Win)
```

## Task tracking

Tasks tracked in-repo via [Backlog.md](https://github.com/MrLesk/Backlog.md). Run `backlog board` to view the kanban.

## License

[MIT](./LICENSE).

The bundled Parakeet model weights are provided by NVIDIA under [CC-BY-4.0](https://creativecommons.org/licenses/by/4.0/). Attribution to NVIDIA is included in-app and in the distribution.
