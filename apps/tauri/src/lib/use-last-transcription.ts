import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  DICTATION_TICK_EVENT,
  PHASE_DONE,
  PHASE_IDLE,
  PHASE_TRANSCRIBING,
  type DictationTick,
  type DictionaryReplacement,
} from "./dictation";
import { NO_REFINE, type RefineInfo } from "./refine";

export interface LatestTranscription {
  text: string;
  replacements: DictionaryReplacement[];
  refine: RefineInfo;
  timestamp: number;
  confidence: number;
}

// Whether this tick is a dictation finishing with text to show:
// the phase just left TRANSCRIBING (or arrived at DONE), and the
// recording was a dictation. An ask recording ends on DONE too, with
// the transcript of the dictation before it still on the tick, so it
// must not count: its question went to the answer panel.
export function dictationJustFinished(prevPhase: number, t: DictationTick): boolean {
  if (t.session_kind === "ask") return false;
  const finalizing =
    (prevPhase === PHASE_TRANSCRIBING && (t.phase === PHASE_DONE || t.phase === PHASE_IDLE)) ||
    (t.phase === PHASE_DONE && prevPhase !== PHASE_DONE);
  return finalizing && t.transcript.trim().length > 0;
}

export function useLastTranscription(): LatestTranscription | null {
  const [latest, setLatest] = useState<LatestTranscription | null>(null);
  const prevPhaseRef = useRef<number>(PHASE_IDLE);

  useEffect(() => {
    const unlisten = listen<DictationTick>(DICTATION_TICK_EVENT, (event) => {
      const t = event.payload;
      if (dictationJustFinished(prevPhaseRef.current, t)) {
        setLatest({
          text: t.transcript,
          replacements: t.replacements ?? [],
          refine: t.refine ?? NO_REFINE,
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
