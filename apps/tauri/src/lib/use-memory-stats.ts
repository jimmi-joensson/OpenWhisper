import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// Mirrors `openwhisper_core::telemetry::ProcessMemory`. Bytes; ms epoch.
export interface ProcessMemory {
  rss_bytes: number;
  peak_rss_bytes: number;
  timestamp_unix_ms: number;
}

export type LifecycleState =
  | "Unloaded"
  | "Loading"
  | "Loaded"
  | "Active"
  | "Releasing";

// Mirrors `openwhisper_core::telemetry::ModelMemoryRow`.
export interface ModelMemoryRow {
  label: string;
  state: LifecycleState;
  estimated_rss_bytes: number;
}

// Mirrors `openwhisper_core::telemetry::MemoryStats`.
export interface MemoryStats {
  process: ProcessMemory;
  models: ModelMemoryRow[];
}

export interface ModelStateChangedPayload {
  label: string;
  state: LifecycleState;
}

const POLL_MS = 1000;
// 60 samples × 1 Hz = 60-second sparkline window. Matches the design's
// "Last 60 s" caption.
export const RSS_SERIES_LEN = 60;

interface UseMemoryStatsResult {
  stats: MemoryStats | null;
  rssSeries: number[];
  error: string | null;
}

// Poll telemetry_get_memory at 1 Hz and listen for model-state-changed
// events to refetch instantly between polls (so a Loading→Loaded
// transition shows up without waiting up to a second). Maintains a
// 60-sample rolling window of process.rss_bytes for the sparkline.
//
// Used by DiagnosticsPane (TASK-62.8). The hook intentionally lives in
// `lib/` rather than inlined into the pane so a future Settings →
// Models pane (per the design) can reuse the same poll without a
// second telemetry stream.
export function useMemoryStats(): UseMemoryStatsResult {
  const [stats, setStats] = useState<MemoryStats | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [rssSeries, setRssSeries] = useState<number[]>([]);
  const seriesRef = useRef<number[]>([]);

  useEffect(() => {
    let cancelled = false;

    const fetchStats = async () => {
      try {
        const next = await invoke<MemoryStats>("telemetry_get_memory");
        if (cancelled) return;
        setStats(next);
        setError(null);

        const ring = seriesRef.current;
        ring.push(next.process.rss_bytes);
        if (ring.length > RSS_SERIES_LEN) {
          ring.splice(0, ring.length - RSS_SERIES_LEN);
        }
        setRssSeries([...ring]);
      } catch (e) {
        if (cancelled) return;
        setError(String(e));
      }
    };

    void fetchStats();
    const id = window.setInterval(fetchStats, POLL_MS);

    let unlisten: UnlistenFn | undefined;
    void listen<ModelStateChangedPayload>("model-state-changed", () => {
      void fetchStats();
    }).then((fn) => {
      if (cancelled) {
        fn();
      } else {
        unlisten = fn;
      }
    });

    return () => {
      cancelled = true;
      window.clearInterval(id);
      unlisten?.();
    };
  }, []);

  return { stats, rssSeries, error };
}
