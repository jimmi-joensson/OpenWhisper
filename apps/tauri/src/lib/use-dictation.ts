import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { PillStatus } from "./pill-state";
import {
  DICTATION_TICK_EVENT,
  PHASE_TRANSCRIBING,
  type DictationTick,
} from "./dictation";

const MAIN_BAR_COUNT = 32;

export interface DictationView {
  status: PillStatus;
  levels: number[];
  elapsed: number;
  samples: number;
  transcript: string;
  confidence: number;
  statusMessage: string;
  errorMessage: string;
  canToggle: boolean;
  toggle: () => Promise<void>;
  cancel: () => Promise<void>;
}

const INITIAL: Omit<DictationView, "toggle" | "cancel"> = {
  status: "idle",
  levels: new Array(MAIN_BAR_COUNT).fill(0),
  elapsed: 0,
  samples: 0,
  transcript: "",
  confidence: 0,
  statusMessage: "",
  errorMessage: "",
  canToggle: true,
};

export function useDictation(): DictationView {
  const [state, setState] = useState(INITIAL);

  useEffect(() => {
    const unlisten = listen<DictationTick>(DICTATION_TICK_EVENT, (event) => {
      const t = event.payload;
      setState((prev) => {
        // Only push level samples while there is meaningful audio activity
        // (recording or the transcribing tail). Idle keeps the buffer flat.
        const next = prev.levels.slice(1);
        next.push(
          t.status === "recording"
            ? t.level
            : t.phase === PHASE_TRANSCRIBING
              ? prev.levels[prev.levels.length - 1] * 0.85
              : 0,
        );
        return {
          status: t.status,
          levels: next,
          elapsed: t.elapsed_ms / 1000,
          samples: t.sample_count,
          transcript: t.transcript,
          confidence: t.confidence,
          statusMessage: t.status_message,
          errorMessage: t.error_message,
          canToggle: t.can_toggle,
        };
      });
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, []);

  const toggle = useCallback(async () => {
    await invoke("dictation_toggle");
  }, []);

  const cancel = useCallback(async () => {
    await invoke("dictation_cancel");
  }, []);

  return { ...state, toggle, cancel };
}
