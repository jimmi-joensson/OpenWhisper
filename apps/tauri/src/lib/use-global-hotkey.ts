// JS-side global-hotkey fallback for the case where OW's own WebView2
// window has focus. The Rust WH_KEYBOARD_LL hook reliably catches the
// chord when *other* apps are focused, but Chromium-in-process registers
// raw keyboard input via RegisterRawInputDevices and that pipeline
// outranks the LL hook for events targeted at the focused process —
// documented at tauri-apps/tauri#13919 / #14770. So when the WebView is
// the focus target the LL hook never fires, and we need a second path.
//
// This hook installs a window-level capture-phase keydown listener.
// Capture-phase + window-level (not document, not React synthetic) so we
// see the event before any <input>/<button> can stopPropagation. The
// handler matches the configured toggle/cancel chords against the event
// and invokes the matching dictation command, calling preventDefault +
// stopImmediatePropagation so Chromium's built-in shortcuts (Ctrl+J →
// downloads, Ctrl+P → print, Ctrl+F → find, Ctrl+R → reload) don't fire
// when the user binds to those.
//
// Gated to Windows. macOS uses CGEventTap which DOES capture events when
// our own window is focused, so installing this fallback there would
// double-toggle on every chord press.
//
// Capture mode (Settings → Shortcuts rebind): module-level shared state
// lets the Settings UI request that the next eligible keydown become a
// captured chord descriptor instead of triggering a toggle. The Rust
// path's `settings_capture_hotkey_start` covers the unfocused case; this
// covers the (much more common) in-focus case where the user is clicking
// "press keys…" inside the Settings pane.

import { useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type HotkeyKind = "modifier-tap" | "chord";
export interface HotkeyConfig {
  kind: HotkeyKind;
  code: string;
  mods: string[];
}
export interface HotkeySettings {
  toggle: HotkeyConfig;
  cancel: HotkeyConfig;
}
export type HotkeyTarget = "toggle" | "cancel";
export interface HotkeyCapturedPayload {
  target: HotkeyTarget;
  config: HotkeyConfig;
}

// ---- module-level capture state ----
//
// Set by Settings when the user clicks "press keys…", consumed by the
// keydown listener on the next non-modifier press. Lives at module scope
// (not React state) so the imperative listener inside the hook closure
// can read the current value without re-subscribing on every change.
type CaptureCallback = (payload: HotkeyCapturedPayload) => void;
let activeCapture: { target: HotkeyTarget; deliver: CaptureCallback } | null = null;

export function startJsCapture(target: HotkeyTarget, deliver: CaptureCallback) {
  activeCapture = { target, deliver };
}

export function cancelJsCapture() {
  activeCapture = null;
}

export function isJsCaptureActive(): boolean {
  return activeCapture !== null;
}

interface UseGlobalHotkeyOpts {
  /** Cancel binding only fires when this is true — mirrors the Rust LL
   *  hook's gate so Esc still works normally inside OW when idle. */
  isRecording: boolean;
}

export function useGlobalHotkey({ isRecording }: UseGlobalHotkeyOpts) {
  const bindingsRef = useRef<HotkeySettings | null>(null);
  const isRecordingRef = useRef(isRecording);
  isRecordingRef.current = isRecording;

  // Detect platform once. Static for the lifetime of the renderer.
  const isWindows =
    typeof navigator !== "undefined" && /win/i.test(navigator.platform);

  // Load + track the current bindings. Refreshes on `hotkey_captured`
  // (fired by the Rust LL hook for the unfocused case) so the JS-side
  // cache stays in sync.
  useEffect(() => {
    if (!isWindows) return;
    let alive = true;
    void invoke<HotkeySettings>("settings_get_hotkeys").then((s) => {
      if (alive) bindingsRef.current = s;
    });

    let unlisten: UnlistenFn | undefined;
    void listen<HotkeyCapturedPayload>("hotkey_captured", (evt) => {
      const cur = bindingsRef.current;
      if (cur) {
        bindingsRef.current = {
          ...cur,
          [evt.payload.target]: evt.payload.config,
        };
      }
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      alive = false;
      unlisten?.();
    };
  }, [isWindows]);

  useEffect(() => {
    if (!isWindows) return;

    function onKeyDown(e: KeyboardEvent) {
      if (e.isComposing) return; // CJK IME mid-composition

      // Capture mode wins — the user is rebinding, not toggling.
      if (activeCapture) {
        if (isModifierCode(e.code)) return;
        const config = chordFromEvent(e);
        if (!config) return;
        const { target, deliver } = activeCapture;
        activeCapture = null;
        e.preventDefault();
        e.stopImmediatePropagation();
        deliver({ target, config });
        return;
      }

      const bindings = bindingsRef.current;
      if (!bindings) return;

      if (chordMatches(bindings.toggle, e)) {
        e.preventDefault();
        e.stopImmediatePropagation();
        void invoke("dictation_toggle");
        return;
      }
      if (chordMatches(bindings.cancel, e)) {
        // Mirror the Rust LL hook's gate: cancel binding only fires
        // while a recording is active. Otherwise we pass through so
        // Esc still closes OW's own menus / browser autofill / etc.
        if (!isRecordingRef.current) return;
        e.preventDefault();
        e.stopImmediatePropagation();
        void invoke("dictation_cancel");
      }
    }

    window.addEventListener("keydown", onKeyDown, { capture: true });
    return () =>
      window.removeEventListener("keydown", onKeyDown, { capture: true });
  }, [isWindows]);
}

// ---- chord matching helpers ----

function chordMatches(cfg: HotkeyConfig, e: KeyboardEvent): boolean {
  if (cfg.kind !== "chord") return false;
  const code = eventCodeToChordName(e.code);
  if (code !== cfg.code) return false;
  const has = (m: string) => cfg.mods.includes(m);
  const ctrlNeed = has("Ctrl") || has("Control");
  const altNeed = has("Alt") || has("Option");
  const shiftNeed = has("Shift");
  const metaNeed = has("Win") || has("Cmd");
  return (
    e.ctrlKey === ctrlNeed &&
    e.altKey === altNeed &&
    e.shiftKey === shiftNeed &&
    e.metaKey === metaNeed
  );
}

function chordFromEvent(e: KeyboardEvent): HotkeyConfig | null {
  const code = eventCodeToChordName(e.code);
  if (!code) return null;
  const mods: string[] = [];
  if (e.ctrlKey) mods.push("Ctrl");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  // Windows-only path: meta = Win key. We don't normalize to "Cmd"
  // because the Mac CGEventTap path emits "Cmd" itself.
  if (e.metaKey) mods.push("Win");
  return { kind: "chord", code, mods };
}

function isModifierCode(code: string): boolean {
  return (
    code === "ControlLeft" ||
    code === "ControlRight" ||
    code === "ShiftLeft" ||
    code === "ShiftRight" ||
    code === "AltLeft" ||
    code === "AltRight" ||
    code === "MetaLeft" ||
    code === "MetaRight" ||
    code === "OSLeft" ||
    code === "OSRight" ||
    code === "CapsLock" ||
    code === "NumLock" ||
    code === "ScrollLock"
  );
}

// Map KeyboardEvent.code to the chord-name format the Rust backend
// expects (must match `chord_name_to_vk` in
// apps/tauri/src-tauri/src/hotkey/windows.rs and `chord_name_to_vk` in
// hotkey/mac.rs). Returns null for keys we don't currently support
// binding to.
function eventCodeToChordName(code: string): string | null {
  if (code.startsWith("Key") && code.length === 4) {
    // "KeyA" → "A"
    return code.charAt(3);
  }
  if (code.startsWith("Digit") && code.length === 6) {
    // "Digit1" → "1"
    return code.charAt(5);
  }
  if (/^F([1-9]|1[0-2])$/.test(code)) {
    return code; // "F1".."F12" pass through
  }
  switch (code) {
    case "Space":
      return "Space";
    case "Tab":
      return "Tab";
    case "Enter":
      return "Return";
    case "Escape":
      return "Escape";
    // Backspace ↔ "Delete" (Rust maps "Delete" to VK_BACK = 0x08).
    // Delete (the forward-delete key) ↔ "ForwardDelete" (VK_DELETE).
    case "Backspace":
      return "Delete";
    case "Delete":
      return "ForwardDelete";
    case "ArrowLeft":
      return "ArrowLeft";
    case "ArrowRight":
      return "ArrowRight";
    case "ArrowUp":
      return "ArrowUp";
    case "ArrowDown":
      return "ArrowDown";
    default:
      return null;
  }
}
