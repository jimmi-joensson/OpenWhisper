# Installing OpenWhisper

macOS **15 Sequoia or later**, Apple Silicon (M-series) only. Intel Macs and macOS 14 (Sonoma) not supported.

Builds on the [Releases page](https://github.com/jimmi-joensson/OpenWhisper/releases) are **signed with a Developer ID and notarized by Apple** — Gatekeeper recognizes them as trusted on first launch, no warnings, no manual bypass.

---

## 1. Download and install

1. Grab the latest `OpenWhisper-<version>-arm64.dmg` from [Releases](https://github.com/jimmi-joensson/OpenWhisper/releases).
2. Double-click the DMG to mount it.
3. Drag **OpenWhisper** into the `Applications` folder.
4. Eject the DMG.

## 2. First launch

Double-click **OpenWhisper** in `/Applications`. macOS verifies Apple's notarization ticket (offline; no network call needed) and launches the app.

The app starts as a **menu-bar agent** — no Dock icon. Look for the OpenWhisper icon in the top-right menu bar.

## 3. Grant permissions

OpenWhisper needs two permissions. Both are prompted automatically — grant Accessibility first; the Microphone prompt only fires after the global hotkey installs successfully.

### Accessibility (prompted)

Needed for the Right Command hotkey and to paste transcribed text into the focused app.

1. On first hotkey press, OpenWhisper shows the macOS *"OpenWhisper would like to control this computer"* dialog. Click **Open System Settings**.
2. In **Privacy & Security → Accessibility**, toggle **OpenWhisper** on.
3. Click **Restart** in the OpenWhisper banner (or quit + relaunch from `/Applications`). macOS only sees the new grant on a fresh process.

### Microphone (prompted)

After Accessibility is granted and the app has restarted, macOS asks for mic access automatically. Click **Allow**.

If missed: **System Settings → Privacy & Security → Microphone** → toggle OpenWhisper on, then relaunch.

## 4. First use

1. Click the OpenWhisper menu-bar icon to confirm it's running.
2. Place your cursor in any text field (Notes, Slack, browser — anything).
3. **Press and release Right Command** — a pill overlay appears. You're recording.
4. Speak.
5. **Press and release Right Command again** — recording stops. A few moments later, your transcribed text appears in the focused field.

### First-run model download

First transcription triggers a one-time download of the Parakeet v3 CoreML weights (hundreds of MB). Subsequent runs are instant and fully local.

Progress is logged — if you want to watch it:
```sh
log stream --predicate 'subsystem == "com.openwhisper.OpenWhisper"' --level debug
```

## Troubleshooting

**Hotkey doesn't do anything, or text doesn't paste.** Accessibility not granted, or OpenWhisper wasn't relaunched after the grant. Check System Settings → Privacy & Security → Accessibility, toggle OpenWhisper on, then quit + relaunch.

**No mic prompt fires after granting Accessibility.** The mic prompt is gated on hotkey-install success — if Accessibility shows toggled on but the hotkey still doesn't work, quit OpenWhisper, then `tccutil reset Accessibility com.openwhisper.app` in Terminal and relaunch to start the prompt cycle fresh.

**Where are logs?** Console.app → search for `com.openwhisper.OpenWhisper` subsystem. Transcript text is redacted by default.

## Uninstall

```sh
rm -rf /Applications/OpenWhisper.app
```

Permissions linger in System Settings until you remove them manually (`-` button in each Privacy & Security list).
