---
name: openwhisper-platform-gotchas
description: Platform-specific behaviors and prior regressions in OpenWhisper's Tauri shell. READ before touching input handling (global hotkeys, keyboard hooks), focus management, audio capture, text injection, or any code that crosses the Rust/WebView boundary differently between Windows and macOS. Each entry below was earned by a real bug — not speculation.
---

# OpenWhisper platform gotchas

Quirks that have actually broken the app, with the fix and the citation. Append new entries as new bugs are discovered; do not delete entries even after the upstream issue is fixed (they document why the workaround is in the tree).

---

## Windows

### WebView2 bypasses `WH_KEYBOARD_LL` when our own window is focused

**Symptom:** Global hotkey (default Ctrl+Space) toggles dictation when any *other* app has focus, but does nothing when OpenWhisper's main window is focused. Verbose logs from inside the LL hook callback confirm: events stop arriving the moment OW gains focus, resume the moment it loses focus. Same pattern for Esc cancel and for the Settings → Shortcuts capture flow.

**Root cause:** Chromium-in-process registers raw keyboard input via `RegisterRawInputDevices`. That pipeline outranks `WH_KEYBOARD_LL` for events targeted at the focused process — so when the WebView is the focus target, the LL hook chain is bypassed entirely. Microsoft's own [`LowLevelKeyboardProc` docs](https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc) note that raw input "can asynchronously monitor mouse and keyboard messages targeted for other threads more effectively than low level hooks." This is intrinsic to in-process Chromium hosting on Windows; it is **not** a Tauri or our-code bug.

Open Tauri issues with the same root cause:
- [tauri-apps/tauri#13919](https://github.com/tauri-apps/tauri/issues/13919) (Jul 2025) — `WH_KEYBOARD_LL` not capturing when Tauri window focused
- [tauri-apps/tauri#14770](https://github.com/tauri-apps/tauri/issues/14770) (Jan 2026) — rdev events stop when Tauri main window focused (mouse OK)

**Fix in tree:** `apps/tauri/src/lib/use-global-hotkey.ts` — a Windows-gated, window-level, capture-phase `keydown` listener that mirrors the configured bindings and calls `dictation_toggle` / `dictation_cancel` + `preventDefault() + stopImmediatePropagation()`. The two paths (Rust LL hook + JS keydown) are mutually exclusive on Windows: LL hook for everything-except-OW-focused, JS handler for OW-focused. Settings → Shortcuts capture also dual-arms both paths via `startJsCapture` so rebinding works whether OW is focused or not.

**macOS is unaffected:** `CGEventTap` (in `apps/tauri/src-tauri/src/hotkey/mac.rs`) captures events even when our own window is focused. The JS handler is gated on `/win/i.test(navigator.platform)` to avoid double-toggle on Mac. If you ever loosen that gate, double-fire WILL happen on Mac.

**Do NOT reinstate the LL hook watchdog.** A previous attempt at fixing this added a 3 s `SetWindowsHookEx`/`UnhookWindowsHookEx` reinstall watchdog (the rationale was "stay at the head of the per-process LL hook chain"). Under sustained churn this corrupted the kernel input thread state on Windows: stuck modifier flags + scancode reordering ("Esc" typed as "D", Ctrl always-on, scroll-zoom permanently active) **surviving process exit**. Recovery required signing out of the Windows user session — quitting the app was not enough. The watchdog was reverted in the same session it was added. The doc-comment at the top of `apps/tauri/src-tauri/src/hotkey/windows.rs` records this. If you find yourself wanting to "just defend the chain order", read that comment first.

---

### Custom titlebar requires `set_decorations(false)` AND four window IPCs in capabilities

**Symptom:** OS title bar still drawn above the app's drawn header strip on Windows even after `decorations: false`. Or: WindowControls (min/max/close) clicks silently no-op — the buttons render, the JS click handler fires, but nothing happens to the window. Identical to the macOS drag-region capability bug above (silent IPC rejection).

**Root cause:** `set_decorations(false)` only removes the OS chrome. The custom min/max/close buttons invoke Tauri 2's `plugin:window|minimize` / `toggle_maximize` / `close` / `is_maximized` IPCs from React. Without `core:window:allow-minimize`, `allow-toggle-maximize`, `allow-close`, and `allow-is-maximized` in `apps/tauri/src-tauri/capabilities/default.json`, the IPC calls reject silently at the capability layer. There is no UI feedback for a denied IPC call — the `invoke` Promise just rejects, and the WindowControls component's `onClick={() => void win.minimize()}` swallows it. Same trap as the macOS `allow-start-dragging` omission documented above.

There's a secondary trap on the maximize/restore icon swap: Tauri 2.10's global `listen("tauri://resize", …)` does NOT fire reliably for synthetic resize events triggered by `toggleMaximize()`. The reliable subscription is `getCurrentWindow().onResized(cb)` — window-scoped, returns the unlisten fn directly.

**Fix in tree:**
1. `apps/tauri/src-tauri/capabilities/default.json` lists all four `core:window:allow-{minimize,toggle-maximize,close,is-maximized}` permissions explicitly.
2. `apps/tauri/src-tauri/src/lib.rs::setup()` calls `main.set_decorations(false)` behind `#[cfg(target_os = "windows")]`. Tauri 2 keeps `WS_THICKFRAME` on the window style after this, so Aero-snap (Win+arrows, edge snap, snap-assist) keeps working — we do not strip it ourselves.
3. `apps/tauri/src/components/window-controls.tsx` subscribes via `getCurrentWindow().onResized(cb)` to keep the maximize/restore icon in sync, NOT global `listen("tauri://resize", …)`.

**macOS is unaffected:** the `cfg(target_os = "windows")` gate compiles to nothing on Mac. `titleBarStyle: "Overlay"` keeps drawing AppKit traffic-lights over the sidebar; the WindowControls component returns `null` on Mac (`platform !== "windows"` early-return). The unified inset titlebar layout (sidebar from y=0, titlebar inset over content column) lands on both platforms identically.

**Do NOT** move `decorations: false` into `tauri.conf.json` — `app.windows[]` in `tauri.dev.conf.json` replaces (not merges) the base array, so platform-conditional decorations would have to be duplicated four times across two configs. The Rust `cfg` gate in `setup()` is cleaner.

**Do NOT** strip `WS_THICKFRAME` manually thinking "decorations: false should kill it" — Tauri keeps it intentionally for Aero-snap. Stripping breaks `Win+←/→/↑/↓` and edge snap-assist, which are baseline Windows expectations.

**Do NOT** switch macOS's `titleBarStyle` to `"Transparent"` to dodge the cfg gate — see the existing macOS drag entry above (loses the focus-loss blur on traffic-lights, which the identity tokens depend on).

**Do NOT** use lucide-react's `Copy` icon as a "restore-down" glyph — `Copy` is a clipboard icon (one rectangle behind another, with a bend), not the Win 11 chrome restore convention (two squares overlapping at the corner). Hand-roll the SVGs in `window-controls.tsx`; lucide doesn't ship a Win 11 chrome glyph set.

---

### BT A2DP↔HFP profile flip has no user-mode "live state" signal — blind sleep is the only fix

**Symptom:** Bluetooth output device (AirPods, BT headphones). Recording opens the mic; the OS forces the BT link from A2DP/stereo into HFP/mono. After the mic closes, sending `TryPlayAsync` to resume the SMTC source (Spotify, browser, etc.) immediately makes the user hear 1–3 s of music in mono before BT actually switches back to stereo. The mic-open / HFP-engage half is unavoidable (BT spec); the user-visible bug is the **resume-too-early** half — we want to delay `TryPlayAsync` until BT is back on A2DP, but there's no user-mode way to know when that is.

**Root cause:** The signal we'd want — "is the BT codec currently in A2DP-active or HFP-active state at this exact moment?" — lives in `BTHHFP.sys`'s IRP queue, surfaced only via `IOCTL_BTHHFP_DEVICE_GET_CONNECTION_STATUS_UPDATE` and `KSEVENT_PINCAPS_JACKINFOCHANGE` ([HFP Device Connection — MS Learn](https://learn.microsoft.com/en-us/windows-hardware/drivers/audio/hfp-device-connection)). Both are kernel-mode-driver-only. There is no user-mode equivalent. Confirmed via three failed code attempts and two research passes:

1. Polling **plain `IAudioClient::GetMixFormat` sample rate** — that's the engine's shared-mode mix format, decoupled from the BT codec layer. Stuck at 48 kHz across the profile flip.
2. Polling **`IAudioClient2 + AudioCategory_Communications + GetMixFormat` channel count** — Microsoft Learn's [Communications Audio Format Capabilities](https://learn.microsoft.com/en-us/windows/win32/coreaudio/communications-audio-format-capabilities) doc claims this reflects live BT codec state on Win11 unified endpoints. **Doc is wrong in practice on Win11 26200 + AirPods Pro.** Verbose-log evidence: value stuck at 1 (HFP-1ch capability) for the entire 3 s timeout window, both at pause-time and resume-time, regardless of whether music was actively playing in A2DP/stereo through the same endpoint. It is a capability query, not a state reflection.
3. **`IMMNotificationClient::OnDefaultDeviceChanged` / `OnDeviceStateChanged` / `OnPropertyValueChanged`** — Win11 unifies A2DP/HFP into one IMMDevice with a stable ID ([Bluetooth Classic Audio — Windows drivers](https://learn.microsoft.com/en-us/windows-hardware/drivers/bluetooth/bluetooth-classic-audio)), so none fire on profile flips. (Win10 had separate endpoints; not relevant for this codebase's targets.)
4. **`Windows.Devices.Bluetooth.HandsFreeProfile.ConnectionStatus`** — service-connected state, not transport-active state. Both A2DP and HFP services stay connected across the flip.
5. **`IPolicyConfig` / `IPolicyConfigVista`** (the undocumented MS interfaces used by EarTrumpet, SoundSwitch) — reads the same property store available to public user-mode APIs ([EarTrumpet IPolicyConfig.cs](https://github.com/File-New-Project/EarTrumpet/blob/dev/EarTrumpet/Interop/MMDeviceAPI/IPolicyConfig.cs)). The property doesn't exist there.
6. **SMTC `PlaybackInfoChanged` / `MediaPropertiesChanged`** — Spotify et al. are upstream of WASAPI, no idea what BT codec is downstream. Their event surface is title/artist/playback-status, nothing transport-related ([GlobalSystemMediaTransportControlsSessionPlaybackInfo](https://learn.microsoft.com/en-us/uwp/api/windows.media.control.globalsystemmediatransportcontrolssessionplaybackinfo)).
7. **ETW `Microsoft.Windows.Bluetooth.BthA2DP`** ({DDB6DA39-08A7-4579-8D0C-68011146E205}) DOES carry profile-flip events — but real-time consumption requires `SeSystemProfilePrivilege` (admin), disqualified for a shipped end-user app.

Industry consensus: **nobody solves this deterministically.** Discord/Teams/Zoom Windows clients all suffer the same mono-tail; the standing user-side workaround is to disable Hands-free Telephony on the BT device ([shkspr.mobi blog](https://shkspr.mobi/blog/2023/09/better-bluetooth-sound-quality-on-microsoft-teams-in-windows-11/)). Wispr Flow's audio docs explicitly tell users to manually pause music ([Wispr Flow audio docs](https://docs.wisprflow.ai/articles/8533503284-knwon-audio-playback-airpod-issues-ios-macos)). Microsoft's Windows team is walking away from the A2DP/HFP split entirely by pushing LE Audio ([MS Windows Platform blog](https://techcommunity.microsoft.com/blog/windowsosplatform/cutting-the-wire-without-cutting-the-audio-quality/4447942)) — i.e., they're not solving the user-mode signal in the legacy stack, just replacing the legacy stack.

**Fix in tree:** `apps/tauri/src-tauri/src/media_control/windows.rs`. `is_default_render_bluetooth()` checks `PKEY_Device_EnumeratorName` for `"BTHENUM"` (Classic) / `"BTHLEDEVICE"` (LE Audio) — that part is deterministic and reliable. On match, `resume_now` does a fixed `thread::sleep(BT_RESUME_DELAY_MS)` (default 5000) before `TryPlayAsync`. Wired/USB endpoints (`USB`/`HDAUDIO`/etc.) skip the sleep entirely — zero added latency. The 5 s value was tuned empirically on AirPods Pro on Win11 26200 — 3 s and 4 s both left audible mono tail-end on consecutive recordings (BT codec stays warmer in HFP after repeated mic cycles and takes longer to drop back). Configurability tracked under TASK-61.8.

**macOS is unaffected:** CoreAudio's `kAudioDevicePropertyNominalSampleRate` IS a live state signal that reflects the actual BT codec, so `apps/tauri/src-tauri/src/media_control/mac.rs::resume_now` polls it deterministically and exits as soon as the rate climbs back to the captured A2DP value. The Windows audio stack is structurally different — no equivalent user-mode property exists. Don't try to "unify" the two implementations; the platform asymmetry is intrinsic, not accidental.

**Do NOT** burn another iteration trying to find a user-mode "live BT codec state" signal. Three approaches and two research passes confirmed there is none. The next attempt will also fail.

**Do NOT** poll `IAudioClient2 + AudioCategory_Communications + GetMixFormat` thinking the MS Learn doc is right — field tests on AirPods Pro/Win11 show the value stuck at HFP-capability regardless of codec state. The doc claim ("supported audio formats... change... when the device is used in Communications mode") is true for capability discovery; it does NOT track live profile transitions.

**Do NOT** rely on `IMMNotificationClient::OnDefaultDeviceChanged` / `OnDeviceStateChanged` callbacks for profile flip detection on Win11 — the unified endpoint model means they don't fire.

**Do NOT** add an ETW listener for `Microsoft.Windows.Bluetooth.BthA2DP` thinking it's "just user-mode tracing" — it requires `SeSystemProfilePrivilege` (admin elevation), which OpenWhisper does not and should not have.

**Do NOT** assume a faster delay value works just because Windows boot was clean. The mono tail gets longer on consecutive recordings — second-recording-in-a-row keeps BT warmer in HFP. Always tune to the worst-case observed, not the first-recording case.

---

## macOS

### Window drag silently no-ops without `core:window:allow-start-dragging`

**Symptom:** With a custom React titlebar in the main window, the window refuses to drag from anywhere inside the strip. Only a 1 px sliver near the OS resize edge moves it. Adding `data-tauri-drag-region` does nothing. Adding a custom `mousedown → invoke('plugin:window|start_dragging')` handler also does nothing. Restarting the dev build doesn't change anything. There is no error in the console.

**Root cause (the real one):** Tauri 2's `core:window:default` permission set does NOT include `allow-start-dragging`. The drag.js handler bundled with Tauri 2.10 calls `invoke('plugin:window|start_dragging')`, and a custom shim has to do the same — both IPCs are silently rejected at the capability layer when the permission is missing. There is no UI feedback for a denied IPC call (the `invoke` Promise just rejects, and drag.js doesn't surface that). See `~/.cargo/registry/src/index.crates.io-*/tauri-2.10.3/permissions/window/autogenerated/reference.md` — the "Default Permission" block lists every allow-* the default set includes; `allow-start-dragging` is conspicuously not in it.

There is a *secondary* macOS issue that compounds the symptom: WKWebView's default `acceptsFirstMouse(for:) = false` makes the webview swallow the first `NSLeftMouseDown` after a focus change. With both blockers present, even fixing one leaves drag broken. Both must be addressed.

Open Tauri issues for the symptom (note: most discussions land on `acceptFirstMouse` as the fix without realising the capability is also missing — the issue templates pre-fill capabilities differently and hide the second blocker):
- [tauri-apps/tauri#9503](https://github.com/tauri-apps/tauri/issues/9503) — Cannot drag tauri app window on macOS when titleBarStyle is Overlay
- [tauri-apps/tauri#11605](https://github.com/tauri-apps/tauri/issues/11605) — can't drag a `data-tauri-drag-region` element when the window is not focused
- [tauri-apps/tauri#4316](https://github.com/tauri-apps/tauri/issues/4316) — Configure `acceptsFirstMouse`
- [tauri-apps/tauri#9901](https://github.com/tauri-apps/tauri/issues/9901) — children inside a drag region cannot fire their own click events

**Fix in tree:**
1. `apps/tauri/src-tauri/capabilities/default.json` lists `core:window:allow-start-dragging` explicitly. Without this, no drag attempt fires regardless of any other config.
2. Both `apps/tauri/src-tauri/tauri.conf.json` and `apps/tauri/src-tauri/tauri.dev.conf.json` set `"acceptFirstMouse": true` on the main window. Don't forget the dev overlay — `app.windows[]` replaces, doesn't merge.
3. `apps/tauri/src/App.tsx` puts `data-tauri-drag-region` on the header strip and on the h1 (drag.js does NOT walk ancestors, so each descendant must opt in). The back button has `data-tauri-drag-region="false"` so its `onClick` still fires.

**Windows impact:** `acceptFirstMouse` is macOS-only. The capability is required everywhere — Windows builds also need `allow-start-dragging` or drag is broken there too.

**Do NOT** rely on `acceptFirstMouse` alone if you find this skill via a search for "drag broken". Always check the capabilities file first — the missing permission is the load-bearing fix; the macOS flag is the secondary one. Also do NOT add a custom JS `mousedown → start_dragging` shim hoping to bypass drag.js: it routes through the same IPC and hits the same capability block, plus on macOS it can't construct a fresh `NSEventType::LeftMouseDown` from inside the webview anyway. Don't switch `titleBarStyle` to `Transparent` to dodge the issue — transparent loses the focus-loss blur AppKit gives the traffic-light row, which is the visual cue our identity tokens depend on.

---

### Plain NSWindow can't render over another app's fullscreen Space

**Symptom:** The pill window has `alwaysOnTop: true` (Tauri → `NSFloatingWindowLevel`) and we set `NSWindowCollectionBehaviorCanJoinAllSpaces | NSWindowCollectionBehaviorFullScreenAuxiliary | NSWindowCollectionBehaviorStationary` via objc2 `msg_send`, plus bumped the level to `NSStatusWindowLevel`. On Sonoma+ the pill still does not appear when another app is in fullscreen — it stays trapped on its origin Space and the fullscreen Space owner draws clean. Hotkey + paste flow still work; only the HUD doesn't render.

**Root cause:** `fullScreenAuxiliary`'s documented contract is "may be shown on the same Space as the fullscreen window," not "above it" — see [Apple Developer Forums #26677](https://developer.apple.com/forums/thread/26677). Several panel-shape behaviors are silently ignored when set on `NSWindow` since Big Sur; the only reliable cross-app fullscreen-overlay path is to make the window a real `NSPanel` (subclass with `NSWindowStyleMask::nonactivatingPanel`) via objc class swizzle (`object_setClass`). Tauri's webview windows are `NSWindow`-backed by default; tao's `set_visible_on_all_workspaces` only flips the `canJoinAllSpaces` bit and doesn't touch the window class. Tauri's own issue tracker logs the same symptom ([tauri#11791](https://github.com/tauri-apps/tauri/issues/11791), closed as "use a panel"). Every shipping Tauri/Electron HUD that overlays cross-app fullscreen (Cap, Screenpipe, Hyprnote, Wispr Flow) goes through an NSPanel swizzle.

**Fix in tree:** `apps/tauri/src-tauri/Cargo.toml` pulls `tauri-nspanel` (Mac-only, git dep on the `v2.1` branch). `apps/tauri/src-tauri/src/lib.rs` declares a `PillPanel` config via `tauri_panel!{}`, registers `tauri_nspanel::init()` on the builder, and converts the pill to a panel in `setup()` before any collection-behavior call (Floating level + nonactivating style mask). `apps/tauri/src-tauri/src/behavior.rs::apply_collection_behavior` drives the panel's `set_collection_behavior(can_join_all_spaces | full_screen_auxiliary)` when `behavior.show_in_fullscreen=true`, empty when false.

**Windows impact:** None. Windows virtual desktops use a different model and `alwaysOnTop` already handles borderless-fullscreen overlay; exclusive-fullscreen DirectX is unsolvable from user-mode regardless. The `tauri-nspanel` dep is gated to `[target.'cfg(target_os = "macos")'.dependencies]`.

**Do NOT** burn another iteration tweaking collection-behavior bits or window levels on a plain `NSWindow` if the symptom is "pill doesn't appear over another app's fullscreen". Two attempts (the `set_visible_on_all_workspaces` + objc2 `fullScreenAuxiliary` combo, then bumping to `NSStatusWindowLevel` + `Stationary`) confirmed both fail on Sonoma+ in this codebase before we found the real fix. The class swizzle is non-negotiable. Also do NOT hand-roll the swizzle in our own objc2 — `tauri-nspanel` is maintained, used by multiple shipping apps, and gives us free updates if Apple changes the underlying NSPanel ABI; rolling it ourselves trades ~30 lines we own for the same risk surface and no upside.

---

### Ad-hoc-signed apps drift their TCC identity on every rebuild — and `codesign -dvv` won't tell you the cdhash

**Status (post-TASK-12, 2026-05-01):** Release builds are now Developer ID-signed + Apple-notarized (`Developer ID Application: Jimmi Joensson (898R9M89GU)`). TCC keys grants on Team Identifier when present, so cdhash drift no longer affects Release upgrades — `0.5.0 → 0.6.0` keeps grants. **This entry now applies only to Debug builds**, which remain ad-hoc-signed via `tauri build --debug`. The boot-time cdhash reset (`version_reset.rs`) still runs on Release for safety / one-time cleanup of pre-signed-release installs but is effectively a no-op once Team ID is the trust anchor; can be retired once we're confident no users are upgrading from a pre-0.5.0 ad-hoc build.

**Symptom (Debug builds, and Release upgrades from a pre-0.5.0 ad-hoc build):** User updates from one release to the next (e.g. 0.3.0 → 0.4.0, or even one 0.4.0 build to a re-signed 0.4.0 build). System Settings → Privacy & Security → Accessibility (and Microphone, and Input Monitoring) still shows the previous "OpenWhisper" entry toggled on, but the global hotkey + mic prompts re-fire as if the grant had never been given. Toggling the existing entry off/on doesn't help. Manually deleting the entry then re-granting works — but having to walk the user through that on every release is the actual UX bug.

**Root cause:** TCC identifies ad-hoc-signed apps (no Apple Developer ID → no Team Identifier) by their **cdhash**. Bundle id stays stable across rebuilds; cdhash does not. Re-signing a freshly compiled binary produces a fresh cdhash even when source is unchanged (the build is deterministic, so a clean `cargo build` of identical sources gives an identical cdhash — but any real source byte change flips it). System Settings reads the row by bundle id and shows it as "still granted", but TCC's `kTCCAccessAuth` lookup keys on cdhash + bundle id, sees a mismatch, and treats the new binary as a fresh identity. The only path that anchors TCC across rebuilds is a paid Apple Developer ID (Team Identifier overrides cdhash); without it, every release looks new to TCC.

A subtle secondary trap when reading own cdhash from Rust: `codesign -dvv` shows `CodeDirectory` but **not** the `CDHash=` line. `CDHash=` only appears at `-dvvvv` (`--verbose=4`) and above. A naive parser using `-dvv` returns None on every boot and silently skips the reset cycle. Verified: `CandidateCDHash sha256=…` is also present at v=4 but uses a different separator and won't match a `strip_prefix("CDHash=")`.

**Fix in tree:** `apps/tauri/src-tauri/src/permissions/version_reset.rs` — TASK-48. On boot (release builds only), shell out to `codesign -d --verbose=4 $current_exe`, parse `CDHash=<hex>` from stderr, compare to the marker file at `~/Library/Application Support/com.openwhisper.app/tcc-last-cdhash`. On mismatch (or marker absent — first install), shell out to `tccutil reset Accessibility|Microphone|ListenEvent com.openwhisper.app` and write the new cdhash. Wired in `lib.rs::setup()` before `hotkey::install` so the AX prompt that follows lands on a fresh row. tccutil exit code is intentionally ignored — exit 1 "no entries to reset" is the desired no-op on a clean install.

**Windows impact:** None. Windows has no equivalent TCC service for keyboard input or mic; the OS handles consent at first device open without an explicit per-bundle grant table.

**Do NOT** key the marker on `CFBundleShortVersionString`. The first iteration of TASK-48 did exactly that and missed the within-version-rebuild case (two 0.4.0 release builds during release prep both wrote "0.4.0" to the marker → second install's reset never fired). cdhash is the only key that matches what TCC itself uses. Also do NOT add a `tccutil reset Microphone` (no bundle id — wipes every app's mic grant) as a "stronger" reset; bundle-id form is correct, the entry truly does clear, and System Settings sometimes shows a stale UI row for a few seconds after the underlying TCC.db row is gone (close + reopen the pane to refresh). Also do NOT bother with self-signed certificates as a "stable identity" workaround — TCC keys grants on Team Identifier when present, and self-signed certs without Apple-issued Team IDs still fall back to cdhash, which still drifts. **The forward path landed in TASK-12** — paid Developer ID + notarization gives Release builds a stable Team Identifier, which is what TCC actually uses.

---

## Cross-platform interface contracts

When adding to this file, prefer the format:

1. **Symptom** (what the user sees / what the logs show)
2. **Root cause** (why, with citations)
3. **Fix in tree** (file paths so the workaround is findable)
4. **Other-platform impact** (if any — e.g. "Mac is unaffected because…")
5. **Don't-do** (anti-patterns that have been tried and failed)

The "don't-do" section is the most valuable bit. Future-you will be tempted to retry the obvious fix.
