# Installing OpenWhisper

macOS **15 Sequoia or later**, Apple Silicon (M-series) only. Intel Macs and macOS 14 (Sonoma) not supported.

Builds on the [Releases page](https://github.com/jimmi-joensson/OpenWhisper/releases) are **ad-hoc signed, not notarized** — free distribution, zero Apple Developer Program cost. macOS Gatekeeper will warn on first launch. The bypass takes ~30 seconds, once per install.

---

## 1. Download and install

1. Grab the latest `OpenWhisper-<version>-arm64.dmg` from [Releases](https://github.com/jimmi-joensson/OpenWhisper/releases).
2. Double-click the DMG to mount it.
3. Drag **OpenWhisper** into the `Applications` folder.
4. Eject the DMG.

## 2. First launch — Gatekeeper bypass

macOS Sequoia (15) and later blocks unsigned apps from opening via double-click. You need one of the flows below.

### Recommended: System Settings flow

1. Double-click **OpenWhisper** in `/Applications`. macOS shows a dialog:
   > `"OpenWhisper" can't be opened because Apple cannot check it for malicious software.`

   Click **Done**.
2. Open **System Settings → Privacy & Security**.
3. Scroll to the bottom. Under the *Security* section you'll see:
   > `"OpenWhisper" was blocked to protect your Mac.`

   Click **Open Anyway**.
4. Authenticate with Touch ID or password.
5. A final confirmation dialog appears — click **Open**.

The app now launches as a **menu-bar agent** (no Dock icon). Look for the OpenWhisper icon in the top-right menu bar.

### Alternative: command line

For the impatient, strip the quarantine flag before first launch:

```sh
xattr -dr com.apple.quarantine /Applications/OpenWhisper.app
open /Applications/OpenWhisper.app
```

## 3. Grant permissions

OpenWhisper needs two permissions. Both are prompted automatically — grant Accessibility first; the Microphone prompt only fires after the global hotkey installs successfully.

### Accessibility (prompted)

Needed for the Right Command hotkey and to paste transcribed text into the focused app.

1. On first launch, OpenWhisper shows the macOS *"OpenWhisper would like to control this computer"* dialog. Click **Open System Settings**.
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

**"App is damaged and can't be opened".** The DMG lost its signature metadata in transit (e.g. re-zipped by email/Slack). Re-download from Releases, or run `xattr -dr com.apple.quarantine /Applications/OpenWhisper.app`.

**Permissions keep getting revoked on updates.** Known pain point for ad-hoc signed builds — every new build has a different signature hash, so macOS TCC treats it as a "different app" and wipes grants. Re-grant after each update. Going away when the project moves to Developer ID signing.

**Where are logs?** Console.app → search for `com.openwhisper.OpenWhisper` subsystem. Transcript text is redacted by default.

## Uninstall

```sh
rm -rf /Applications/OpenWhisper.app
```

Permissions linger in System Settings until you remove them manually (`-` button in each Privacy & Security list).
