import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { emitTo } from "@tauri-apps/api/event";
import { MainWindowShell, type Platform } from "./components/main-window-shell";
import { useDictation } from "./lib/use-dictation";
import { useHotkeyStatus } from "./lib/use-hotkey-status";
import { usePermissionsStatus } from "./lib/use-permissions-status";
import { PILL_STATE_EVENT, type PillState } from "./lib/pill-state";
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
  const platform = detectPlatform();
  const dictation = useDictation();
  const hotkey = useHotkeyStatus();
  const permissions = usePermissionsStatus();

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
  }, []);

  // Forward last 12 levels to the pill window so it mirrors this window's
  // amplitude envelope.
  useEffect(() => {
    void emitTo("pill", PILL_STATE_EVENT, {
      status: dictation.status,
      levels: dictation.levels.slice(-PILL_BAR_COUNT),
    } satisfies PillState);
  }, [dictation.status, dictation.levels]);

  return (
    <MainWindowShell
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
  );
}

export default App;
