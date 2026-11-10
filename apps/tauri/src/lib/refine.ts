// Types for the Refine step (TASK-93). Mirror core/src/settings/refine.rs,
// core/src/refine/mod.rs and src-tauri/src/refine.rs.

export type RefineBackend = "claude-cli" | "openai-compatible";

// How hard the model thinks before answering. Mirrors `RefineEffort`
// in core: the levels the backends take, not a recommendation.
export type RefineEffort = "minimal" | "low" | "medium" | "high" | "xhigh" | "max";

export interface RefineSettings {
  enabled: boolean;
  backend: RefineBackend;
  // Empty = the built-in default instructions.
  instructions: string;
  timeout_ms: number;
  min_words: number;
  drift_guard: boolean;
  claude_cli: { model: string; effort: RefineEffort; path: string | null };
  // `effort: null` = no effort is sent (most servers do not take one).
  openai: { base_url: string; model: string; effort: RefineEffort | null };
}

export interface RefineState {
  settings: RefineSettings;
  default_instructions: string;
  test_sentence: string;
}

export type RefineBackendState =
  | "ready"
  | "not-installed"
  | "not-signed-in"
  | "not-configured"
  | "unknown";

export interface RefineBackendStatus {
  backend: RefineBackend;
  state: RefineBackendState;
  ready: boolean;
  // One plain sentence: what is wrong and what to do, or that all is well.
  message: string;
  // Where dictated text is sent with this backend.
  destination: string;
  path: string | null;
  version: string | null;
  auth_method: string | null;
  plan: string | null;
  has_api_key: boolean;
}

export interface RefineConnectionTest {
  ok: boolean;
  backend: string;
  output: string;
  message: string;
  elapsed_ms: number;
}

// Mirrors `RefineInfo` in core/src/dictation.rs: what Refine did to the
// transcript on a dictation tick. `status` is "" when Refine did not run.
export interface RefineInfo {
  raw_transcript: string;
  status:
    | ""
    | "refined"
    | "skipped-short"
    | "timeout"
    | "unavailable"
    | "failed"
    | "rejected";
  detail: string;
  // The spoken instruction Refine carried out ("make that more
  // formal"), in the speaker's words; "" for plain cleanup.
  instruction: string;
  backend: string;
  elapsed_ms: number;
}

export const NO_REFINE: RefineInfo = {
  raw_transcript: "",
  status: "",
  detail: "",
  instruction: "",
  backend: "",
  elapsed_ms: 0,
};

// Mirrors `RefineRun` in core/src/refine/history.rs. One Refine run;
// timings and reasons only, never any dictated text.
export interface RefineRun {
  at_ms: number;
  source: "dictation" | "test";
  status: RefineInfo["status"];
  backend: string;
  model: string;
  effort: string;
  elapsed_ms: number;
  // ms the backend was started ahead of the request; null = cold.
  warm_lead_ms: number | null;
  chars_in: number;
  chars_out: number;
  instruction: boolean;
  detail: string;
  // What the call used. null when the backend did not say (or the
  // run predates this data).
  tokens_in: number | null;
  tokens_cached: number | null;
  tokens_out: number | null;
  // API list price: a yardstick for size, not a charge on a plan.
  cost_usd: number | null;
  // Share (0 to 1) of the plan's windows used right after the call.
  window_5h_used: number | null;
  window_5h_resets_at: number | null;
  window_7d_used: number | null;
  window_7d_resets_at: number | null;
}

// Mirrors `PlanEstimate` in core/src/refine/history.rs. The windows
// count all use of the plan, so `max_share_per_run` is an upper bound
// for one dictation and `full_at_ms` is for the plan as a whole.
export interface RefinePlanEstimate {
  as_of_ms: number;
  five_hour_used: number;
  five_hour_resets_at: number | null;
  seven_day_used: number | null;
  seven_day_resets_at: number | null;
  window_runs: number;
  window_moved: number | null;
  max_share_per_run: number | null;
  full_at_ms: number | null;
  pace_known: boolean;
}

// Mirrors `RefineSummary` in core/src/refine/history.rs.
export interface RefineSummary {
  total: number;
  refined: number;
  skipped: number;
  fallbacks: number;
  timeouts: number;
  unavailable: number;
  failed: number;
  rejected: number;
  instructions: number;
  median_ms: number | null;
  p90_ms: number | null;
  max_ms: number | null;
  median_warm_ms: number | null;
  median_cold_ms: number | null;
  warm_runs: number;
  cold_runs: number;
  last_problem_at_ms: number | null;
  last_problem: string;
  usage_runs: number;
  avg_tokens_in: number | null;
  avg_tokens_out: number | null;
  avg_cost_usd: number | null;
  total_cost_usd: number | null;
  plan: RefinePlanEstimate | null;
}

export interface RefineHistory {
  summary: RefineSummary;
  runs: RefineRun[];
}

// True when a backend was asked and its answer was not used, so the
// unrefined transcript was inserted.
export function refineFellBack(info: RefineInfo): boolean {
  return (
    info.status === "timeout" ||
    info.status === "unavailable" ||
    info.status === "failed" ||
    info.status === "rejected"
  );
}
