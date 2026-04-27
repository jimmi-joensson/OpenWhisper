// Mirrors `DictationTick` in src-tauri/src/lib.rs.
export interface DictationTick {
  phase: number;
  status: "idle" | "recording" | "transcribing";
  status_message: string;
  transcript: string;
  confidence: number;
  sample_count: number;
  elapsed_ms: number;
  error_message: string;
  can_toggle: boolean;
  is_recording: boolean;
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
