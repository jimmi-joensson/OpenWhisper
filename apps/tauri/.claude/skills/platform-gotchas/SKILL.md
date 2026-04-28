---
name: platform-gotchas
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

## macOS

*(No documented gotchas yet. Add entries here as they're discovered. Prior macOS-specific learnings are encoded in source-file doc-comments — see `apps/tauri/src-tauri/src/hotkey/mac.rs` for CGEventTap watchdog rationale, `apps/tauri/src-tauri/src/permissions/` for AX/Mic prompt sequencing, `apps/tauri/src-tauri/src/focus.rs` for the AX-grant watcher pattern.)*

---

## Cross-platform interface contracts

When adding to this file, prefer the format:

1. **Symptom** (what the user sees / what the logs show)
2. **Root cause** (why, with citations)
3. **Fix in tree** (file paths so the workaround is findable)
4. **Other-platform impact** (if any — e.g. "Mac is unaffected because…")
5. **Don't-do** (anti-patterns that have been tried and failed)

The "don't-do" section is the most valuable bit. Future-you will be tempted to retry the obvious fix.
