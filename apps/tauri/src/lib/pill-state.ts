export type PillStatus = "idle" | "recording" | "transcribing";

export interface PillState {
  status: PillStatus;
  levels: number[]; // 12 normalized 0..1 floats
}

export const EMPTY_LEVELS: number[] = Array.from({ length: 12 }, () => 0);

export const INITIAL_PILL_STATE: PillState = {
  status: "idle",
  levels: EMPTY_LEVELS,
};

/** Tauri event channel name. Main window emits, pill window listens. */
export const PILL_STATE_EVENT = "pill_state";
