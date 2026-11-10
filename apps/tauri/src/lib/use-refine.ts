import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

import type {
  RefineBackendStatus,
  RefineConnectionTest,
  RefineSettings,
  RefineState,
} from "./refine";

// State and actions for the Refine settings pane. Rust owns everything:
// the settings block, the backend probe, the keychain entry and the
// connection test. This hook only sequences the calls and holds what
// came back.
export function useRefine() {
  const [state, setState] = useState<RefineState | null>(null);
  const [status, setStatus] = useState<RefineBackendStatus | null>(null);
  const [checking, setChecking] = useState(false);
  const [test, setTest] = useState<RefineConnectionTest | null>(null);
  const [testing, setTesting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Drop answers that belong to an older request (the user switched
  // backend while a probe was in flight).
  const statusSeq = useRef(0);

  const refreshStatus = useCallback(async () => {
    const seq = ++statusSeq.current;
    setChecking(true);
    try {
      const next = await invoke<RefineBackendStatus>("refine_status");
      if (seq === statusSeq.current) setStatus(next);
    } catch (e) {
      if (seq === statusSeq.current) setError(String(e));
    } finally {
      if (seq === statusSeq.current) setChecking(false);
    }
  }, []);

  useEffect(() => {
    invoke<RefineState>("refine_get")
      .then((s) => {
        setState(s);
        void refreshStatus();
      })
      .catch((e) => setError(String(e)));
  }, [refreshStatus]);

  // Save a change to the settings block. Renders what Rust stored
  // (trimmed, clamped), then re-checks the backend, since most changes
  // (backend, URL, model) affect whether it can run.
  const update = useCallback(
    async (patch: Partial<RefineSettings>) => {
      if (!state) return;
      const next = { ...state.settings, ...patch };
      // Optimistic, so a switch does not lag behind the click.
      setState({ ...state, settings: next });
      try {
        const saved = await invoke<RefineSettings>("refine_set", { settings: next });
        setState((s) => (s ? { ...s, settings: saved } : s));
        setError(null);
        setTest(null);
        void refreshStatus();
      } catch (e) {
        setState(state);
        setError(String(e));
      }
    },
    [state, refreshStatus],
  );

  const runTest = useCallback(async () => {
    setTesting(true);
    setTest(null);
    try {
      setTest(await invoke<RefineConnectionTest>("refine_test"));
    } catch (e) {
      setError(String(e));
    } finally {
      setTesting(false);
    }
  }, []);

  // Empty key = remove the stored one.
  const setApiKey = useCallback(
    async (key: string) => {
      try {
        await invoke<boolean>("refine_set_api_key", { key });
        setError(null);
        setTest(null);
        await refreshStatus();
        return true;
      } catch (e) {
        setError(String(e));
        return false;
      }
    },
    [refreshStatus],
  );

  return {
    state,
    status,
    checking,
    test,
    testing,
    error,
    update,
    refreshStatus,
    runTest,
    setApiKey,
  };
}
