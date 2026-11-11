// Mirrors `Replacement` in core/src/dictionary.rs.
export interface DictionaryReplacement {
  from: string;
  to: string;
}

import type { RefineInfo } from "./refine";

// Mirrors `SessionKind` in core/src/dictation.rs.
export type SessionKind = "dictation" | "ask";

// Mirrors `DictationTick` in src-tauri/src/lib.rs.
export interface DictationTick {
  phase: number;
  // What the recording in flight is for, or what the last one was
  // for. After an ask recording the phase goes to DONE while
  // `transcript` still holds the dictation before it: an "ask" tick
  // never carries a new dictation.
  session_kind: SessionKind;
  status: "idle" | "recording" | "transcribing";
  status_message: string;
  transcript: string;
  // Dictionary fixes in `transcript`: what the recognizer wrote → the term.
  replacements: DictionaryReplacement[];
  // What the Refine step did to `transcript`, with the unrefined text.
  refine: RefineInfo;
  confidence: number;
  sample_count: number;
  elapsed_ms: number;
  error_message: string;
  can_toggle: boolean;
  is_recording: boolean;
  // The cancel key would do something now: a recording, or an ask
  // that is being transcribed or answered.
  cancel_would_act: boolean;
  level: number;
  download_bytes_done: number;
  download_bytes_total: number;
}

export const DICTATION_TICK_EVENT = "dictation_tick";

// Phase constants — keep in sync with core/src/dictation.rs.
export const PHASE_IDLE = 0;
export const PHASE_LOADING_MODEL = 1;
export const PHASE_RECORDING = 2;
export const PHASE_TRANSCRIBING = 3;
export const PHASE_DONE = 4;
export const PHASE_ERROR = 5;
