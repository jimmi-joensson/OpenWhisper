import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

import type { RefineSettings, RefineState } from "./refine";

// The Refine on/off switch alone, for the quick switch on Home
// (TASK-100). Reads and writes the same settings block as the Refine
// pane, but does not probe the backend: `useRefine` starts the CLI to
// check the sign-in, which is too much to do every time Home opens.
export function useRefineEnabled() {
  const [settings, setSettings] = useState<RefineSettings | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<RefineState>("refine_get")
      .then((s) => setSettings(s.settings))
      .catch((e) => setError(String(e)));
  }, []);

  const setEnabled = useCallback(
    async (enabled: boolean) => {
      if (!settings) return;
      const next = { ...settings, enabled };
      setSettings(next);
      try {
        setSettings(await invoke<RefineSettings>("refine_set", { settings: next }));
        setError(null);
      } catch (e) {
        setSettings(settings);
        setError(String(e));
      }
    },
    [settings],
  );

  return { enabled: settings?.enabled ?? null, error, setEnabled };
}
