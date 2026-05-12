import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen, type UnlistenFn } from "@tauri-apps/api/event";
import { DiagnosticsPane, type Platform } from "./components/diagnostics-pane";
import { HomePane } from "./components/home-pane";
import { DevToolsPanel } from "./components/dev-tools-panel";
import { CrashesLaunchToast } from "./components/crashes-launch-toast";
import { SidebarNav, type Route } from "./components/sidebar-nav";
import { WindowControls } from "./components/window-controls";
import { SettingsShell } from "./Settings";
import type { SettingsPaneId } from "./lib/settings-panes";
import { useDictation } from "./lib/use-dictation";
import { useGlobalHotkey } from "./lib/use-global-hotkey";
import { useHotkeyStatus } from "./lib/use-hotkey-status";
import { usePermissionsStatus } from "./lib/use-permissions-status";
import { usePauseDiagnostic } from "./lib/use-pause-diagnostic";
import { ThemeProvider } from "./lib/use-theme";
import {
  EMPTY_LEVELS,
  PILL_STATE_EVENT,
  type PillState,
  type PillStatus,
} from "./lib/pill-state";
import { PHASE_ERROR } from "./lib/dictation";
import "./App.css";

const PILL_BAR_COUNT = 12;

function detectPlatform(): Platform {
  if (typeof navigator === "undefined") return "macos";
  return /win/i.test(navigator.platform) ? "windows" : "macos";
}

// Centered titlebar text per route. Mirrors the design's
// "OpenWhisper — Settings" / "OpenWhisper — Diagnostics" / plain
// "OpenWhisper" pattern. Sits in the same strip as the traffic
// lights on macOS.
function titlebarTitle(route: Route): string {
  switch (route) {
    case "home":
      return "OpenWhisper";
    case "settings":
      return "OpenWhisper — Settings";
    case "diagnostics":
      return "OpenWhisper — Diagnostics";
  }
}

function App() {
  const [route, setRoute] = useState<Route>("home");
  const [settingsPane, setSettingsPane] = useState<SettingsPaneId>("general");
  const platform = detectPlatform();

  // Leaving Settings resets the pane to General so re-entering always lands
  // on the canonical first pane. Keeps in-Settings nav lossless (clicking
  // around between panes preserves your spot) without making the route exit
  // feel like a partial back.
  useEffect(() => {
    if (route !== "settings" && settingsPane !== "general") {
      setSettingsPane("general");
    }
  }, [route, settingsPane]);

  // Mac sidebar reserves 38 px padding-top for traffic-light overlay; CSS
  // selector keys on body[data-platform="macos"]. Set once on mount.
  useEffect(() => {
    document.body.setAttribute("data-platform", platform);
  }, [platform]);

  const dictation = useDictation();
  const hotkey = useHotkeyStatus();
  const permissions = usePermissionsStatus();
  const pauseDiagnostic = usePauseDiagnostic();

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

  // ⌘, switches the in-window route to Settings. Settings is no longer a
  // separate window — it's a routed view inside the main window, so the
  // keyboard shortcut just flips local state.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const cmdOrCtrl = e.metaKey || e.ctrlKey;
      if (cmdOrCtrl && e.key === ",") {
        e.preventDefault();
        setRoute("settings");
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // Tray Preferences… (and any other Rust-side trigger) emits `ow_navigate`
  // with a target view. Listen here and swap routes. The "main" payload maps
  // to "home" so existing tray-menu wiring keeps working without a Rust change.
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    void listen<string>("ow_navigate", (evt) => {
      if (evt.payload === "settings") {
        setRoute("settings");
      } else if (evt.payload === "main") {
        setRoute("home");
      } else if (evt.payload === "diagnostics") {
        setRoute("diagnostics");
      }
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

  const goBack = useCallback(() => setRoute("home"), []);

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
    <ThemeProvider>
    <div className="ow-app">
      {/* Layout: full-width titlebar at y=0 (matches design — traffic
          lights + centered route title share one strip), then the
          sidebar / content shell below. Drag region —
          `data-tauri-drag-region` on the titlebar + on the h1 (Tauri
          2.10's drag.js only checks `e.target.getAttribute`, not
          ancestors, so descendants must opt in individually). The
          back button explicitly opts OUT via `="false"` so its
          onClick still fires (per tauri#9901). The whole drag flow
          only works because the main window has `acceptFirstMouse:
          true` set in tauri.conf.json — without it, WKWebView
          swallows the first NSLeftMouseDown and AppKit never sees a
          chance to start the window drag (tauri#9503). */}
      <header
        className={`ow-titlebar ow-titlebar--${route}`}
        data-tauri-drag-region
      >
        <h1 className="ow-titlebar__title" data-tauri-drag-region>
          {titlebarTitle(route)}
        </h1>
        <WindowControls platform={platform} />
      </header>
      <div className="ow-app__shell">
        <SidebarNav
          route={route}
          onRouteSelect={setRoute}
          settingsPane={settingsPane}
          onSettingsPaneSelect={setSettingsPane}
        />
        <div className="ow-app__column">
          {/* Boot-time launch notice for unread crashes (TASK-78.5).
              Hidden once the user views or dismisses; suppressed for
              same-or-lower unread restarts via persisted
              last_seen_unread_count. View routes to the Diagnostics
              overview, NOT the inspector — entering the inspector is
              the explicit per-crash read action. */}
          <CrashesLaunchToast onView={() => setRoute("diagnostics")} />
          <main className="ow-app__body">
            {route === "settings" && (
              <SettingsShell active={settingsPane} onBack={goBack} />
            )}
            {route === "diagnostics" && (
              <DiagnosticsPane platform={platform} />
            )}
            {route === "home" && (
              <HomePane
                hotkeyError={hotkey.status && !hotkey.status.ok ? hotkey.status.error : null}
                onHotkeyRetry={() => void hotkey.retry()}
                micError={
                  permissions.status && !permissions.status.mic_ok
                    ? permissions.status.error
                    : null
                }
                onMicOpenSettings={() => {
                  void invoke("open_microphone_settings").catch(() => {});
                }}
                automationError={
                  pauseDiagnostic?.reason === "not_authorized"
                    ? "Automation permission denied — paused music won't resume after dictation."
                    : null
                }
                onAutomationOpenSettings={() => {
                  void invoke("open_automation_settings").catch(() => {});
                }}
                recognizerError={recognizerError}
              />
            )}
          </main>
        </div>
      </div>
      {/* Dev tools — persistent floating trigger across every route.
          DEV-gated at the call site so release builds compile it
          out entirely. */}
      {import.meta.env.DEV && (
        <DevToolsPanel
          pillEnabled={pillOverride.enabled}
          pillStatus={pillOverride.status}
          onPillEnabledChange={(enabled) =>
            setPillOverride((prev) => ({ ...prev, enabled }))
          }
          onPillStatusChange={(status) =>
            setPillOverride((prev) => ({ ...prev, status }))
          }
        />
      )}
    </div>
    </ThemeProvider>
  );
}

export default App;
