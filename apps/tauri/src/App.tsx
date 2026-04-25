import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { emitTo } from "@tauri-apps/api/event";
import { MainWindowShell, type Platform } from "./components/main-window-shell";
import { useDictation } from "./lib/use-dictation";
import { PILL_STATE_EVENT, type PillState } from "./lib/pill-state";
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
      status={dictation.status}
      levels={dictation.levels}
      elapsed={dictation.elapsed}
      samples={dictation.samples}
      transcript={dictation.transcript || (dictation.status === "idle" ? "—" : "…")}
      platform={platform}
      onToggle={() => void dictation.toggle()}
      coreVersion={coreVersion}
      coreError={coreError}
    />
  );
}

export default App;
