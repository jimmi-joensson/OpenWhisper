import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getName } from "@tauri-apps/api/app";
import { MainWindowShell, type Platform } from "./components/main-window-shell";
import { DevPillControls } from "./components/dev-pill-controls";
import { SettingsShell } from "./Settings";
import { useDictation } from "./lib/use-dictation";
import { useGlobalHotkey } from "./lib/use-global-hotkey";
import { useHotkeyStatus } from "./lib/use-hotkey-status";
import { usePermissionsStatus } from "./lib/use-permissions-status";
import {
  EMPTY_LEVELS,
  PILL_STATE_EVENT,
  type PillState,
  type PillStatus,
} from "./lib/pill-state";
import { PHASE_ERROR } from "./lib/dictation";
import "./App.css";

type View = "main" | "settings";

const PILL_BAR_COUNT = 12;

function detectPlatform(): Platform {
  if (typeof navigator === "undefined") return "macos";
  return /win/i.test(navigator.platform) ? "windows" : "macos";
}

function App() {
  const [coreVersion, setCoreVersion] = useState<string | null>(null);
  const [coreError, setCoreError] = useState<string | null>(null);
  const [view, setView] = useState<View>("main");
  // App productName from Tauri runtime — "OpenWhisper" (release) or
  // "OpenWhisper Dev" (per tauri.dev.conf.json overlay). Single source of
  // truth: the running bundle's CFBundleName.
  const [appName, setAppName] = useState<string>("OpenWhisper");
  const platform = detectPlatform();
  const dictation = useDictation();
  const hotkey = useHotkeyStatus();
  const permissions = usePermissionsStatus();

  // Windows-only fallback for the WebView2-focused case where the Rust
  // WH_KEYBOARD_LL hook is bypassed by Chromium's raw-input registration
  // (tauri-apps/tauri#13919). No-op on macOS — CGEventTap already
  // captures in-focus events. Cancel binding gates on isRecording so Esc
  // still works normally inside OW when idle.
  useGlobalHotkey({ isRecording: dictation.isRecording });

  // Dev-only pill state override. When `enabled`, the auto-emit from the
  // dictation hook is suppressed and we drive the pill from the floating
  // controls (with a simulated 20 Hz envelope for the "recording" state).
  const [pillOverride, setPillOverride] = useState<{
    enabled: boolean;
    status: PillStatus;
  }>({ enabled: false, status: "idle" });

  // Recognizer-load failure: surfaced via the dictation phase machine
  // (`dictation_deliver_error` flips phase to ERROR with a "recognizer
  // load" prefix). Per-utterance transcribe failures keep using the
  // small "last error" KV row in the debug card — only boot/load
  // failures get the full banner because the recovery is different
  // (relaunch vs. record again).
  const recognizerError =
    dictation.phase === PHASE_ERROR &&
    dictation.errorMessage.startsWith("recognizer load")
      ? dictation.errorMessage
      : null;

  useEffect(() => {
    invoke<string>("core_version")
      .then(setCoreVersion)
      .catch((e) => setCoreError(String(e)));
    getName()
      .then(setAppName)
      .catch(() => {
        // Keep default "OpenWhisper" if the API isn't available.
      });
  }, []);

  // ⌘, switches the in-window route to Settings. Settings is no longer a
  // separate window — it's a routed view inside the main window, so the
  // keyboard shortcut just flips local state.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const cmdOrCtrl = e.metaKey || e.ctrlKey;
      if (cmdOrCtrl && e.key === ",") {
        e.preventDefault();
        setView("settings");
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // Tray Preferences… (and any other Rust-side trigger) emits `ow_navigate`
  // with a target view. Listen here and swap routes.
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    void listen<string>("ow_navigate", (evt) => {
      if (evt.payload === "settings" || evt.payload === "main") {
        setView(evt.payload);
      }
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

  const goBack = useCallback(() => setView("main"), []);

  // Auto-emit pill state from the dictation hook. Skipped while the dev
  // override is active so manual selections aren't immediately overwritten
  // by a state ping from the core.
  useEffect(() => {
    if (pillOverride.enabled) return;
    void emitTo("pill", PILL_STATE_EVENT, {
      status: dictation.status,
      levels: dictation.levels.slice(-PILL_BAR_COUNT),
    } satisfies PillState);
  }, [dictation.status, dictation.levels, pillOverride.enabled]);

  // Manual override emitter. Idle/transcribing emit once; recording emits
  // a simulated envelope at 20 Hz so the pill meter has data without a mic.
  useEffect(() => {
    if (!pillOverride.enabled) return;

    if (pillOverride.status !== "recording") {
      void emitTo("pill", PILL_STATE_EVENT, {
        status: pillOverride.status,
        levels: EMPTY_LEVELS,
      } satisfies PillState);
      return;
    }

    let levels = new Array<number>(PILL_BAR_COUNT).fill(0);
    const id = setInterval(() => {
      const t = performance.now() / 1000;
      const env = 0.45 + 0.35 * Math.sin(t * 1.7) + 0.18 * Math.sin(t * 4.3);
      const noise = (Math.random() - 0.5) * 0.4;
      const v = Math.max(0.005, Math.min(1, env * Math.abs(env) + noise * 0.6));
      levels = [...levels.slice(1), v];
      void emitTo("pill", PILL_STATE_EVENT, {
        status: "recording",
        levels,
      } satisfies PillState);
    }, 50);
    return () => clearInterval(id);
  }, [pillOverride.enabled, pillOverride.status]);

  return (
    <div className="ow-app">
      {/* Drag region — `data-tauri-drag-region` on the strip + on the
          h1 (Tauri 2.10's drag.js only checks `e.target.getAttribute`,
          not ancestors, so descendants must opt in individually). The
          back button explicitly opts OUT via `="false"` so its onClick
          still fires (per tauri#9901). The whole drag flow only works
          because the main window has `acceptFirstMouse: true` set in
          tauri.conf.json — without it, WKWebView swallows the first
          NSLeftMouseDown and AppKit never sees a chance to start the
          window drag (tauri#9503). */}
      <header
        className={`ow-titlebar ow-titlebar--${view}`}
        data-tauri-drag-region
      >
        {view === "settings" && (
          <>
            <button
              type="button"
              className="ow-titlebar__back"
              onClick={goBack}
              aria-label="Back to main"
              data-tauri-drag-region="false"
            >
              <span aria-hidden="true">←</span>
            </button>
            <h1 className="ow-titlebar__title" data-tauri-drag-region>
              Settings
            </h1>
          </>
        )}
      </header>
      <main className="ow-app__body">
        {view === "settings" ? (
          <SettingsShell />
        ) : (
          <>
            <MainWindowShell
              title={appName}
              phase={dictation.phase}
              status={dictation.status}
              levels={dictation.levels}
              level={dictation.level}
              elapsed={dictation.elapsed}
              samples={dictation.samples}
              transcript={dictation.transcript}
              confidence={dictation.confidence}
              statusMessage={dictation.statusMessage}
              errorMessage={dictation.errorMessage}
              canToggle={dictation.canToggle}
              isRecording={dictation.isRecording}
              downloadBytesDone={dictation.downloadBytesDone}
              downloadBytesTotal={dictation.downloadBytesTotal}
              platform={platform}
              onToggle={() => void dictation.toggle()}
              coreVersion={coreVersion}
              coreError={coreError}
              hotkeyError={hotkey.status && !hotkey.status.ok ? hotkey.status.error : null}
              onHotkeyRetry={() => void hotkey.retry()}
              micError={
                permissions.status && !permissions.status.mic_ok
                  ? permissions.status.error
                  : null
              }
              recognizerError={recognizerError}
            />
            {import.meta.env.DEV && (
              <DevPillControls
                enabled={pillOverride.enabled}
                status={pillOverride.status}
                onToggle={(enabled) =>
                  setPillOverride((prev) => ({ ...prev, enabled }))
                }
                onStatus={(status) =>
                  setPillOverride((prev) => ({ ...prev, status }))
                }
              />
            )}
          </>
        )}
      </main>
    </div>
  );
}

export default App;
