import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  DICTATION_TICK_EVENT,
  PHASE_DONE,
  PHASE_IDLE,
  PHASE_TRANSCRIBING,
  type DictationTick,
} from "./dictation";

export interface LatestTranscription {
  text: string;
  timestamp: number;
  confidence: number;
}

export function useLastTranscription(): LatestTranscription | null {
  const [latest, setLatest] = useState<LatestTranscription | null>(null);
  const prevPhaseRef = useRef<number>(PHASE_IDLE);

  useEffect(() => {
    const unlisten = listen<DictationTick>(DICTATION_TICK_EVENT, (event) => {
      const t = event.payload;
      const prev = prevPhaseRef.current;
      const finalizing =
        (prev === PHASE_TRANSCRIBING && (t.phase === PHASE_DONE || t.phase === PHASE_IDLE)) ||
        (t.phase === PHASE_DONE && prev !== PHASE_DONE);
      if (finalizing && t.transcript.trim().length > 0) {
        setLatest({
          text: t.transcript,
          timestamp: Date.now(),
          confidence: t.confidence,
        });
      }
      prevPhaseRef.current = t.phase;
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, []);

  return latest;
}
