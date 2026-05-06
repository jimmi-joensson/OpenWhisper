import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface StatsSettings {
  user_wpm: number;
}

export const USER_WPM_MIN = 10;
export const USER_WPM_MAX = 300;
export const DEFAULT_USER_WPM = 40;

export const SETTINGS_STATS_CHANGED_EVENT = "settings_stats_changed";

export interface UseUserWpm {
  /// Resolved typing-speed calibration. Falls back to DEFAULT_USER_WPM
  /// while the initial settings_get_stats invoke is in flight, so the
  /// Time Saved formula always has a non-null number.
  wpm: number;
  /// Persist a new value. Out-of-range writes are silently clamped to
  /// [USER_WPM_MIN, USER_WPM_MAX] by the Rust side; the resolved value
  /// flows back through the settings_stats_changed event.
  setWpm: (next: number) => Promise<void>;
}

export function useUserWpm(): UseUserWpm {
  const [wpm, setWpmState] = useState<number>(DEFAULT_USER_WPM);

  useEffect(() => {
    let alive = true;
    void invoke<StatsSettings>("settings_get_stats").then(
      (s) => {
        if (alive) setWpmState(s.user_wpm);
      },
      // Settings read failure: stay on DEFAULT_USER_WPM. Rust logs the
      // root cause; users still get a usable Time Saved estimate.
      () => {},
    );

    let unlisten: UnlistenFn | undefined;
    void listen<number>(SETTINGS_STATS_CHANGED_EVENT, (evt) => {
      if (alive && typeof evt.payload === "number") {
        setWpmState(evt.payload);
      }
    }).then((fn) => {
      if (alive) unlisten = fn;
      else fn();
    });

    return () => {
      alive = false;
      unlisten?.();
    };
  }, []);

  const setWpm = useCallback(async (next: number) => {
    const stored = await invoke<number>("settings_set_user_wpm", { wpm: next });
    setWpmState(stored);
  }, []);

  return { wpm, setWpm };
}
