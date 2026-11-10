import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import type {
  RefineBackendStatus,
  RefineConnectionTest,
  RefineHistory,
  RefineState,
} from "./refine";

// How many runs the Diagnostics section loads and summarizes.
export const REFINE_HISTORY_LIMIT = 50;

// Data for Diagnostics → Refine: is the backend reachable, how fast
// does it answer, and what happened on recent dictations. All of it
// comes from Rust (`refine_*` commands); the history is the same
// SQLite table `openwhisper refine history` reads.
export function useRefineHistory() {
  const [history, setHistory] = useState<RefineHistory | null>(null);
  const [enabled, setEnabled] = useState<boolean | null>(null);
  const [status, setStatus] = useState<RefineBackendStatus | null>(null);
  const [checking, setChecking] = useState(false);
  const [test, setTest] = useState<RefineConnectionTest | null>(null);
  const [testing, setTesting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadHistory = useCallback(async () => {
    try {
      setHistory(
        await invoke<RefineHistory>("refine_history", { limit: REFINE_HISTORY_LIMIT }),
      );
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const checkBackend = useCallback(async () => {
    setChecking(true);
    try {
      setStatus(await invoke<RefineBackendStatus>("refine_status"));
    } catch (e) {
      setError(String(e));
    } finally {
      setChecking(false);
    }
  }, []);

  useEffect(() => {
    void loadHistory();
    invoke<RefineState>("refine_get")
      .then((s) => {
        setEnabled(s.settings.enabled);
        void checkBackend();
      })
      .catch((e) => setError(String(e)));
    // Each finished dictation writes its Refine run just before the
    // stats row, so the existing "stats changed" signal is the cue.
    const unlisten = listen("stats_changed", () => {
      void loadHistory();
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [loadHistory, checkBackend]);

  const runTest = useCallback(async () => {
    setTesting(true);
    setTest(null);
    try {
      setTest(await invoke<RefineConnectionTest>("refine_test"));
      // The test is recorded as a run too.
      await loadHistory();
    } catch (e) {
      setError(String(e));
    } finally {
      setTesting(false);
    }
  }, [loadHistory]);

  const clear = useCallback(async () => {
    try {
      await invoke("refine_history_clear");
      await loadHistory();
    } catch (e) {
      setError(String(e));
    }
  }, [loadHistory]);

  return {
    history,
    enabled,
    status,
    checking,
    test,
    testing,
    error,
    refresh: loadHistory,
    checkBackend,
    runTest,
    clear,
  };
}
