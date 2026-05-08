Local-first hotkey dictation for macOS and Windows. macOS DMG + Windows MSI / NSIS installers attached.

## What's new

**Home stats strip (TASK-87 / 88 / 89)** — *headline feature*
- New live KV strip on Home: **Dictations today**, **This week**, **All time**, **Time saved** — at-a-glance signal that the app is actually working for you, no more hunting for "did I dictate today?"
- Counters update within ~1 s of every completed dictation via a live `stats_changed` event — no manual refresh, no stale numbers
- **Time saved** is computed from your typing speed: change **Settings → General → Typing speed** and the subcaption updates immediately
- Active-time bucketed per **local midnight** (not UTC) via the new energy-VAD path, so day rollovers feel right regardless of timezone
- Persistence is SQLite-backed (`<app-data>/openwhisper.db`, bundled SQLite + idempotent schema migrations) — survives crashes, upgrades, and OS restarts
- **Settings → General** gains a **Reset stats** action (with confirm) and a Typing-speed input that clamps out-of-range entries

**Crash inspector (TASK-78)**
- Rust panic hook now writes a redacted JSON crash file to `~/Library/Application Support/OpenWhisper/crashes/` (Mac) or `%APPDATA%\OpenWhisper\crashes\` (Windows) on every panic — runtime homes, paths, and other PII are stripped before the file lands on disk
- New Diagnostics → Crashes pane shows a card on the Diagnostics overview and a full crash list in its own route, with relative timestamps and unread state
- Detail sheet renders the redacted backtrace with a one-click **Copy backtrace** (icon morphs Copy → CircleCheck on success), **Open crash folder** (shells out via Command — opener plugin is ignored deliberately), and a primary **Report on GitHub** button that pre-fills an issue with the redacted markdown
- Sidebar rail dot in lockstep with the unread crash list — click into Diagnostics → Crashes and the dot clears as items are marked read
- CLI parity: `openwhisper crash-dump <id>` prints the same redacted markdown the GitHub report would carry

**Diagnostics → Memory pane (TASK-62 foundation)**
- Live RSS sparkline with a fixed Y-ceiling and per-frame interpolation; cubic-Bezier continuous flow that survives `prefers-reduced-motion`
- System memory readout (total / used) alongside total OW process memory
- Polled refresh while open; event-driven refresh on lifecycle state changes
- Backed by a cross-platform process-memory primitive in `core/`

**Diagnostics OpenWhisper Memory Breakdown bar (TASK-62 Stream B)**
- Single platform-aware breakdown bar covering total OpenWhisper memory (process RSS + ANE/GPU claim) — sums match the "OpenWhisper Memory" readout above the bar
- Parakeet segment is sourced from process RSS on Windows (in-process via ort, ~612 MB) and from the ANE/GPU claim on Mac (FluidAudio, ~461 MB), with the legend label disambiguating which side carries the weights
- Segments gate by lifecycle state (Loaded / Loading / Idle) — only loaded models contribute

**Settings → Models — memory budget bar + storage panel (TASK-62 Stream B)**
- Memory budget bar at the top of the pane with a physical-memory readout and a hover-ghost preview that previews the delta of toggling a row before you commit
- Per-row delta chips (add / remove) and a green/amber headroom hint
- Storage panel: model count + on-disk path with a platform-aware reveal button (Show in Finder on Mac, Show in Explorer on Windows)
- Footer caveat below the bar links to Diagnostics for live measurements

**Keep models warm (TASK-62 foundation)**
- New General-pane Switch: when ON, models stay resident across idle windows; when OFF, the idle timer auto-releases the recognizer after a configured deadline
- ModelHandle state machine + idle timer live in `core/`; persisted via `settings_set_keep_models_warm`

**AppleScript Automation TCC surfacing (TASK-82, Mac)**
- When pause-during-dictation hits an AppleScript Automation denial for Spotify or Apple Music, the app now surfaces the TCC denial in the audio-ducking flow instead of failing silently

**Boot permission flow — AX strictly before mic (Mac)**
- Mic prompt is now owned by the AX-grant watcher: it fires on the watcher's boot tick iff Accessibility is already trusted, and on every subsequent false → true edge. Earlier code queued the mic request on the next run-loop tick after `hotkey::install`, which could fire the AVCapture mic dialog while System Settings was still mid-prompt and read as "mic before AX"
- Release builds gate strictly on `AXIsProcessTrusted()`; dev builds keep the `hotkey_status_current` fallback so ad-hoc cdhash drift doesn't lock dev users out of the prompt

**`openwhisper` CLI (TASK-81)**
- New cross-platform CLI binary mirroring the headless library surface — settings, diagnostics, media-gate, crash-dump
- Shipped as a sibling binary (not a UI replacement); the Tauri shell consumes the same library API

## Known limitations

- **Crash inspector**: launch-time toast for unread crashes and bulk-delete are deferred (TASK-78.5 is partial — rail dot shipped). Playwright redaction regression coverage (TASK-78.7) is also outstanding
- **Mac memory breakdown — ANE granularity**: the Apple Neural Engine doesn't expose per-tensor accounting, so Parakeet's ~461 MB ANE claim renders as a single segment. Splitting it further (encoder / decoder / CTC head) would require model-side changes in NeMo/CoreML conversion, not OpenWhisper
- **Audio ducking — browser-tab media** (carried from 0.5.0): Safari / Chrome / Firefox tab playback is still not paused on macOS; the per-app AppleScript route covers Spotify + Apple Music only

## Install

See [INSTALL.md](https://github.com/jimmi-joensson/OpenWhisper/blob/main/INSTALL.md) for setup. macOS DMG is signed + notarized — double-click and drag to Applications. Windows is shipped as both MSI (enterprise / group-policy) and NSIS exe (per-user, friendlier consumer install).
