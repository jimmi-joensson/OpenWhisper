import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface StatsSummary {
  words_today: number;
  words_week: number;
  words_all_time: number;
  /// Sum of dictation duration_ms across all rows, in seconds. Frontend
  /// formats into Time Saved using the WPM setting.
  seconds_total: number;
}

export const STATS_CHANGED_EVENT = "stats_changed";

export interface UseStatsSummary {
  summary: StatsSummary | null;
  refresh: () => void;
}

export function useStatsSummary(): UseStatsSummary {
  const [summary, setSummary] = useState<StatsSummary | null>(null);

  const refresh = useCallback(() => {
    void invoke<StatsSummary>("stats_get_summary").then(
      (s) => setSummary(s),
      // Failure here means the store didn't open (logged on the Rust
      // side). Render empty state rather than blocking the Home pane.
      () => setSummary(null),
    );
  }, []);

  useEffect(() => {
    let alive = true;
    void invoke<StatsSummary>("stats_get_summary").then(
      (s) => {
        if (alive) setSummary(s);
      },
      () => {
        if (alive) setSummary(null);
      },
    );

    let unlisten: UnlistenFn | undefined;
    void listen<unknown>(STATS_CHANGED_EVENT, () => {
      void invoke<StatsSummary>("stats_get_summary").then(
        (s) => {
          if (alive) setSummary(s);
        },
        () => {},
      );
    }).then((fn) => {
      if (alive) unlisten = fn;
      else fn();
    });

    return () => {
      alive = false;
      unlisten?.();
    };
  }, []);

  return { summary, refresh };
}
