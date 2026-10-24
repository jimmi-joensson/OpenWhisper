Security update for Windows. **Replaces 0.6.0**; please update. macOS DMG + Windows MSI / NSIS installers attached.

## Security fix: verified model download (Windows)

On 20 Oct 2026 the private keys of several eMudhra/emSign intermediate certificate authorities were disclosed (Mozilla bug 2083449). Software that still trusts those chains can be served a fake HTTPS certificate by anyone in the network path, for example on public Wi-Fi.

On Windows, OpenWhisper downloads its speech model (~487 MB, from the k2-fsa/sherpa-onnx GitHub releases) on first launch over a TLS stack that used its own bundled root certificates. 0.6.0 trusted the affected roots and extracted the archive without checking it, so a network attacker could in principle have substituted the model archive.

0.6.1 fixes this in two ways:
- **SHA-256 pin on the model archive.** The download is hashed while it streams and rejected before extraction unless it matches the published digest. A previously cached archive is re-checked before extraction and re-downloaded if it does not match.
- **Updated root certificates.** rustls-webpki 0.103.16 and webpki-roots 1.0.10 no longer trust the compromised roots.

**macOS is not affected.** The Mac app gets its model through FluidAudio and the system networking stack, which uses Apple's (already patched) trust store; the affected download code is not compiled on macOS.

**If you are on Windows and want to be thorough:** quit OpenWhisper, delete `%USERPROFILE%\.cache\openwhisper\models`, and start 0.6.1. The model will be downloaded again and verified. An already extracted model is not re-checked automatically.

## Also in this release

**Transcribe audio and video files (TASK-91)**
- New **Transcribe** route: pick or drag-drop a file, watch progress, copy the transcript
- Supports wav, mp3, m4a/aac, flac, aiff, caf, Ogg/WebM Opus (WhatsApp voice notes) and mp4/mov/mkv/webm video, no ffmpeg needed
- Same path headless via `openwhisper transcribe <file>`

## Known limitations

- **File transcription**: Opus decode runs at about 30x realtime, so a 1 h voice memo spends about 2 min in "Reading audio..."; decoded audio is held in memory (~230 MB per hour)
- Limitations listed for 0.6.0 still apply

## Install

See [INSTALL.md](https://github.com/jimmi-joensson/OpenWhisper/blob/main/INSTALL.md) for setup. macOS DMG is signed + notarized; double-click and drag to Applications. Windows is shipped as both MSI (enterprise / group-policy) and NSIS exe (per-user, friendlier consumer install).
