# Models

OpenWhisper does **not** convert model weights at build time. We consume pre-converted CoreML artifacts published by [FluidInference](https://huggingface.co/FluidInference) on Hugging Face.

## Default (MVP, English only)

**[FluidInference/parakeet-tdt-0.6b-v2-coreml](https://huggingface.co/FluidInference/parakeet-tdt-0.6b-v2-coreml)**

- Based on [nvidia/parakeet-tdt-0.6b-v2](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v2)
- License (weights): [CC-BY-4.0](https://creativecommons.org/licenses/by/4.0/) — attribution to NVIDIA required
- Runtime: CoreML, runs on ANE (Apple Neural Engine)
- First-run download size: ~500 MB, stored in `~/Library/Application Support/OpenWhisper/models/`

## Multilingual (future)

**[FluidInference/parakeet-tdt-0.6b-v3-coreml](https://huggingface.co/FluidInference/parakeet-tdt-0.6b-v3-coreml)** — multilingual variant. Opt-in via settings.

## Attribution

The bundled app must surface attribution for:
- **NVIDIA** — original Parakeet weights (CC-BY-4.0)
- **FluidInference / FluidAudio** — Swift library and CoreML conversion (Apache-2.0, carry NOTICE if present)
- **Apple** — CoreML framework

See `docs/spikes/task-3-parakeet-on-apple-silicon.md` for the spike that chose this path.
