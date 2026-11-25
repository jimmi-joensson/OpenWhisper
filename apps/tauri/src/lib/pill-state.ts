export type PillStatus = "idle" | "recording" | "transcribing";

export interface PillState {
  status: PillStatus;
  levels: number[]; // 12 normalized 0..1 floats
  /** The dictation in flight goes through Refine. The pill says
   *  "Refine" above the capsule for as long as this is set and the
   *  pill is not idle: while recording and while transcribing. */
  willRefine?: boolean;
  /** The transcript is with the Refine backend: the pill says
   *  "Refining" above the capsule for as long as this is set. The
   *  capsule itself moves as for any dictation. */
  refining?: boolean;
}

export const EMPTY_LEVELS: number[] = Array.from({ length: 12 }, () => 0);

export const INITIAL_PILL_STATE: PillState = {
  status: "idle",
  levels: EMPTY_LEVELS,
};

/** Tauri event channel name. Main window emits, pill window listens. */
export const PILL_STATE_EVENT = "pill_state";
