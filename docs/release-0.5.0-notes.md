Local-first hotkey dictation for macOS and Windows. macOS DMG + Windows MSI / NSIS installers attached.

## What's new

**Signed + notarized Mac DMG**
- First OpenWhisper release built end-to-end with a Developer ID Application signature and an Apple-notarized + stapled DMG
- Gatekeeper accepts the install on first launch — no manual bypass, no "Apple cannot check it for malicious software" warning
- (Windows installers were already self-served; no install-flow change there)

**Home pane + outer sidebar nav**
- Main window now lands on a dedicated Home pane with a live hotkey hint and a latest-transcript row (hover to copy)
- Outer sidebar nav: Home / Settings / Diagnostics
- Sidebar swaps to Settings panes when you enter Settings, and resets to General on exit

**Windows custom titlebar**
- Slack-style continuous dark panel across sidebar + titlebar
- Native min / max / close rendered in-app; OS chrome dropped
- Close button now sits flush against the right edge

**Audio ducking + pause during dictation**
- Other-app playback pauses (or ducks, configurable) while you dictate, then resumes when the pill closes
- macOS: Spotify and Apple Music are paused via AppleScript; resume waits for Bluetooth headsets to switch back from HFP/mono to A2DP/stereo before unpausing
- Windows: system MediaController (SMTC) drives the pause; user-configurable Bluetooth resume-delay slider in Settings → General handles flaky-headset resume timing
- **Known limitations (macOS):** first time you record while Spotify or Music is playing, macOS shows a one-time Automation permission prompt per app — click Allow once and the prompts don't return. Browser-tab media (Safari/Chrome/Firefox YouTube etc.) is not paused on macOS in this release; tracked for a future release. macOS 15.4 closed the underlying MediaRemote APIs to non-Apple-signed processes, which is why the per-app AppleScript route is the only deterministic path right now

**Pill 2× scale during record / transcribe**
- Spring-driven scale tween on the pill HUD when recording or transcribing
- `prefers-reduced-motion` fallback respected
- Backdrop-blur counter-scale fix so the blur stays crisp through the tween

**Launch at login (wired)**
- The General-pane "Launch at login" Switch now actually persists and auto-starts the app on login (was a UI stub in 0.4.0)
- Backed by `tauri-plugin-autostart`; works on macOS and Windows

**Mac hotkey regrant reliability**
- After re-granting Accessibility, the app re-launches via `open -n` instead of `app.restart()`, which preserves the launchctl registration the system hotkey path relies on. No more "had to quit and relaunch manually after re-granting" loop

**Icon + chrome polish**
- Settings-pane icons + the back arrow now use lucide (replacing the unicode/emoji glyphs)
- Titlebar right padding tightened on Windows so the close button sits flush

## Install

See [INSTALL.md](https://github.com/jimmi-joensson/OpenWhisper/blob/main/INSTALL.md) for setup. macOS DMG is signed + notarized — double-click and drag to Applications. Windows is shipped as both MSI (enterprise / group-policy) and NSIS exe (per-user, friendlier consumer install).
