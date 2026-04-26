import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { emitTo } from "@tauri-apps/api/event";
import { getName } from "@tauri-apps/api/app";
import { MainWindowShell, type Platform } from "./components/main-window-shell";
import { DevPillControls } from "./components/dev-pill-controls";
import { useDictation } from "./lib/use-dictation";
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

const PILL_BAR_COUNT = 12;

function detectPlatform(): Platform {
  if (typeof navigator === "undefined") return "macos";
  return /win/i.test(navigator.platform) ? "windows" : "macos";
}

function App() {
  const [coreVersion, setCoreVersion] = useState<string | null>(null);
  const [coreError, setCoreError] = useState<string | null>(null);
  // App productName from Tauri runtime — "OpenWhisper" (release) or
  // "OpenWhisper Dev" (per tauri.dev.conf.json overlay). Single source of
  // truth: the running bundle's CFBundleName.
  const [appName, setAppName] = useState<string>("OpenWhisper");
  const platform = detectPlatform();
  const dictation = useDictation();
  const hotkey = useHotkeyStatus();
  const permissions = usePermissionsStatus();

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
  );
}

export default App;
