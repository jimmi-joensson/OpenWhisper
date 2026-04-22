# OpenWhisper

Open-source, local-first dictation for macOS. An alternative to Superwhisper where **local models are free** and you bring your own keys for any cloud providers you want to use.

## Why

Existing dictation tools paywall good local transcription. Their free local models mis-spell constantly and force you to hand-curate vocabulary. OpenWhisper runs strong local models (Whisper family, Parakeet, etc.) out of the box at no cost — you only pay if you opt into cloud features with your own API keys, or if you subscribe to future hosted conveniences (e.g. synced settings, managed billing). Core dictation stays free.

## Status

Very early. Project scaffolding in progress. MVP target: **macOS**, using NVIDIA Parakeet (CC-BY-4.0) converted to CoreML for Apple Neural Engine execution. Windows and Linux ports to follow.

## How it works (MVP)

1. Press the activation hotkey — OpenWhisper starts listening and a pill overlay appears.
2. Talk.
3. Press the same hotkey again — recording stops, transcription runs locally on the ANE, and the resulting text is pasted into the currently focused input field.

Hotkey defaults to match Superwhisper's for familiarity; fully rebindable in settings.

## Stack

- **Shared core:** Rust (audio capture, VAD, config, custom vocab, post-processing, BYO-key cloud providers)
- **macOS shell:** Swift + SwiftUI/AppKit, CoreML on Apple Neural Engine
- **Windows shell** (future): C# + WinUI 3, ONNX Runtime + DirectML
- **Linux shell** (future): Rust + gtk4-rs, ONNX Runtime (CUDA/ROCm/CPU)

## Principles

- **Local-first, free by default.** Strong local transcription with no paywall.
- **BYO keys.** Any cloud model integration uses the user's own API credentials.
- **No dark patterns.** Paid tiers only exist for features that genuinely cost us money (hosted sync, subscription infra, etc.), never for gating local capability.
- **Correctable.** Custom vocabulary, post-processing, and prompt shaping are first-class — not afterthoughts.

## Task tracking

Tasks tracked in-repo via [Backlog.md](https://github.com/MrLesk/Backlog.md). Run `backlog board` to view the kanban.

## License

[MIT](./LICENSE).

The bundled Parakeet model weights are provided by NVIDIA under [CC-BY-4.0](https://creativecommons.org/licenses/by/4.0/). Attribution to NVIDIA is included in-app and in the distribution.
