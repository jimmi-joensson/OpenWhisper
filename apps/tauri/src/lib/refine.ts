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
  // Refine and Ask never write an em dash or an en dash. On unless
  // the person switched it off.
  no_long_dashes: boolean;
  claude_cli: { model: string; effort: RefineEffort; path: string | null };
  // `effort: null` = no effort is sent (most servers do not take one).
  openai: { base_url: string; model: string; effort: RefineEffort | null };
}

// The model names the Claude CLI takes as-is. Which of them work
// depends on the user's plan, which the CLI does not let us ask: a
// model the plan lacks shows up as a failed connection test, not as a
// missing menu entry. Offered by the Refine pane and by a mode's own
// model choice; both also take any other full model name.
export const CLAUDE_MODELS = [
  { value: "fable", label: "Fable" },
  { value: "opus", label: "Opus" },
  { value: "sonnet", label: "Sonnet" },
  { value: "haiku", label: "Haiku" },
] as const;

export const EFFORT_LABELS: Record<RefineEffort, string> = {
  minimal: "Minimal",
  low: "Low",
  medium: "Medium",
  high: "High",
  xhigh: "Extra high",
  max: "Max",
};

// What the Refine pane calls each backend, for sentences that name
// one ("for the Claude backend").
export const BACKEND_LABELS: Record<RefineBackend, string> = {
  "claude-cli": "Claude",
  "openai-compatible": "OpenAI-compatible",
};

// The effort levels each kind of backend takes today.
export const BACKEND_EFFORTS: Record<RefineBackend, RefineEffort[]> = {
  "claude-cli": ["low", "medium", "high", "xhigh", "max"],
  "openai-compatible": ["minimal", "low", "medium", "high", "xhigh"],
};

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
  // The model that answered: the exact one when the backend says
  // ("claude-sonnet-5-5"), otherwise as the settings name it
  // ("sonnet"); "" when the backend has no named model. Absent from
  // a tick sent by an older build.
  model?: string;
  // The effort the model was asked for ("low"); "" when none was sent.
  effort?: string;
  elapsed_ms: number;
  // Name of the mode the dictation ran in (TASK-96), when there is
  // something to say about it: a mode the user made, or any mode
  // (Dictation too) that ran because the app has it as its default.
  // "" for Dictation as the selected mode and when Refine did not run.
  mode: string;
  // `mode` is the built-in Dictation mode.
  mode_builtin: boolean;
  // Why that mode ran. "" when Refine did not run.
  mode_reason: "" | "selected" | "app-default";
  // Name of the app whose default mode it was; "" unless
  // `mode_reason` is "app-default".
  mode_app: string;
}

export const NO_REFINE: RefineInfo = {
  raw_transcript: "",
  status: "",
  detail: "",
  instruction: "",
  backend: "",
  elapsed_ms: 0,
  mode: "",
  mode_builtin: false,
  mode_reason: "",
  mode_app: "",
};

// A model's name as a person reads it. A Claude model by family and
// version: "claude-sonnet-5-5" is "Sonnet 5.5", also with a date at
// the end ("claude-opus-4-1-20250805" is "Opus 4.1") or the version
// first ("claude-3-5-sonnet-20241022" is "Sonnet 3.5"); a bare family
// name ("sonnet", as the settings have it) is "Sonnet". Any other
// model is shown as it is written.
export function modelLabel(model: string): string {
  const name = model.trim();
  const short = CLAUDE_MODELS.find((m) => m.value === name.toLowerCase());
  if (short) return short.label;
  if (!/^claude-/i.test(name)) return name;
  const parts = name.toLowerCase().split("-").slice(1);
  const words = parts.filter((p) => /^[a-z]+$/.test(p) && p !== "latest");
  // Version numbers are short; eight digits are a release date.
  const version = parts.filter((p) => /^\d{1,2}$/.test(p));
  if (words.length === 0) return name;
  const family = words.map((w) => w[0].toUpperCase() + w.slice(1)).join(" ");
  return version.length > 0 ? `${family} ${version.join(".")}` : family;
}

// Who refined a text, as a person reads it: the model (`modelLabel`),
// or the backend ("Claude") when no model is known. `by` is
// `RefineInfo.model`, or what a history entry stored
// (`HistoryEntry.refined`: the model, or the backend's id). "" when
// nothing is known.
export function refinedByLabel(by: string | null | undefined, backend = ""): string {
  const name = (by ?? "").trim() || backend.trim();
  if (!name) return "";
  return name in BACKEND_LABELS ? BACKEND_LABELS[name as RefineBackend] : modelLabel(name);
}

// The effort as the Refine settings name it ("Low", "Extra high"),
// for small print where it stands right after the model: "Sonnet 5.5
// · Low". "" for none, and an effort this build does not know is
// shown as written.
export function effortLabel(effort: string | null | undefined): string {
  const id = (effort ?? "").trim();
  if (!id) return "";
  return id in EFFORT_LABELS ? EFFORT_LABELS[id as RefineEffort] : id;
}

// The effort inside a sentence, where a bare "Low" would not read:
// "Refined by Sonnet 5.5 at low effort". "" for none.
export function effortPhrase(effort: string | null | undefined): string {
  const label = effortLabel(effort);
  return label ? `${label.toLowerCase()} effort` : "";
}

// Mirrors `ModelInUse` in core/src/refine/mod.rs: what a dictation
// would be refined with now. `model` is the exact one once a run has
// named it ("claude-sonnet-5-5"), before that the name in the
// settings ("sonnet"); "" when none is set. `effort` is "" when none
// is sent.
export interface RefineModelInUse {
  backend: string;
  model: string;
  effort: string;
}

// Mirrors `RefineRun` in core/src/refine/history.rs. One Refine run;
// timings and reasons only, never any dictated text.
export interface RefineRun {
  at_ms: number;
  // "ask" rows (TASK-97) are asks answered by the same backend, not
  // Refine runs: their status is "answered", "timeout", "unavailable",
  // "failed" or "cancelled".
  source: "dictation" | "test" | "ask";
  status: RefineInfo["status"] | "answered" | "cancelled";
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
  // Name of the mode the run was made in, when it was one the user
  // made. null for the built-in Dictation mode and for older runs.
  mode: string | null;
  // An ask: ms from sending the question to the first text of the
  // answer. null for Refine runs and for asks that got no text.
  first_text_ms: number | null;
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
  // Rows looked at, asks included.
  total: number;
  // Asks among them, how many got an answer, and over the answered
  // ones the median time to the first text and to the end.
  asks: number;
  asks_answered: number;
  ask_median_first_text_ms: number | null;
  ask_median_ms: number | null;
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
