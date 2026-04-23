# Models

OpenWhisper does **not** convert model weights at build time. We consume pre-converted CoreML artifacts published by [FluidInference](https://huggingface.co/FluidInference) on Hugging Face.

## Default (multilingual, 25 languages)

**[FluidInference/parakeet-tdt-0.6b-v3-coreml](https://huggingface.co/FluidInference/parakeet-tdt-0.6b-v3-coreml)**

- Based on [nvidia/parakeet-tdt-0.6b-v3](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3)
- License (weights): [CC-BY-4.0](https://creativecommons.org/licenses/by/4.0/) — attribution to NVIDIA required
- Runtime: CoreML, runs on ANE (Apple Neural Engine)
- First-run download size: ~500 MB, stored in `~/Library/Application Support/OpenWhisper/models/`
- Languages: 25 European (Bulgarian, Croatian, Czech, Danish, Dutch, English, Estonian, Finnish, French, German, Greek, Hungarian, Italian, Latvian, Lithuanian, Maltese, Polish, Portuguese, Romanian, Slovak, Slovenian, Spanish, Swedish, Russian, Ukrainian)
- Auto-detects spoken language per utterance — no user setting required
- Known limitation (upstream): intra-utterance code-switching (mid-sentence language change) may produce transcription errors; per-utterance switching works reliably

## Attribution

The bundled app must surface attribution for:
- **NVIDIA** — original Parakeet weights (CC-BY-4.0)
- **FluidInference / FluidAudio** — Swift library and CoreML conversion (Apache-2.0, carry NOTICE if present)
- **Apple** — CoreML framework

See `docs/spikes/task-3-parakeet-on-apple-silicon.md` for the spike that chose this path.
