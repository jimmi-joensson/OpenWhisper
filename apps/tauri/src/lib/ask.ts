// Types and event names for Ask (TASK-97). Mirror core/src/ask/mod.rs,
// core/src/settings/ask.rs and src-tauri/src/ask.rs. An ask is a
// spoken (or typed) question, answered by the backend Refine is set
// up with and shown in the answer window instead of being typed.

import type { RefineBackendStatus, RefineEffort } from "./refine";

// Mirrors `AskPhase` in core.
export type AskPhase =
  | "idle"
  | "listening"
  | "thinking"
  | "answering"
  | "done"
  | "error"
  | "cancelled";

// Mirrors `ScreenState` in core/src/ask/screen.rs: what became of
// the screen in one ask (Vision, TASK-98). It is a word, never the
// picture: a screenshot does not reach a window.
export type AskScreen =
  // No screenshot in this ask (so far).
  | "none"
  // The screen is being captured right now.
  | "capturing"
  // A screenshot was sent with the question.
  | "included"
  // No screenshot: an app on the never-capture list is in front.
  | "blocked"
  // A screenshot was wanted and could not be taken.
  | "unavailable";

// Mirrors `CallUsage` in core/src/refine/mod.rs: what the call used.
export interface AskUsage {
  // All input tokens, cached ones included.
  tokens_in: number;
  tokens_cached: number;
  tokens_out: number;
  // API list price: a yardstick for size, not a charge on a plan.
  cost_usd: number | null;
  // Claude CLI on a subscription: how full the plan's windows are
  // after the call (shares 0 to 1, resets in Unix epoch seconds).
  plan: {
    five_hour_used: number | null;
    five_hour_resets_at: number | null;
    seven_day_used: number | null;
    seven_day_resets_at: number | null;
  } | null;
}

// Mirrors `AskSnapshot` in core: the one ask of the running app.
// This is the whole payload of `ask_tick` and what `ask_snapshot`
// and `ask_start` return.
export interface AskSnapshot {
  // Tells one ask from the next. 0 before the first.
  id: number;
  phase: AskPhase;
  // The question as it was sent.
  question: string;
  // The answer so far while "answering", the whole answer when
  // "done". Kept when an ask is cancelled or fails midway.
  answer: string;
  notice: string;
  // Why there is no answer; "" unless the phase is "error".
  error: string;
  backend: string;
  model: string;
  // "" when no effort is sent.
  effort: string;
  // Web search is on for this ask.
  web_search: boolean;
  // The model is searching the web right now, and for what.
  searching: boolean;
  search_query: string;
  // When the ask began, Unix epoch milliseconds.
  started_at_ms: number;
  // Since the question was sent. Moves while the model works.
  elapsed_ms: number;
  first_text_ms: number | null;
  usage: AskUsage | null;
  // What became of the screen in this ask.
  screen: AskScreen;
  // Why there is no screenshot, as one sentence for the person; ""
  // unless `screen` is "blocked" or "unavailable".
  screen_notice: string;
  // Goes up on every change.
  version: number;
}

// Emitted when the ask changes, and about four times a second while
// one is in flight. Payload: `AskSnapshot`.
export const ASK_TICK_EVENT = "ask_tick";

// Mirrors `AskSettings` in core: the `ask` block of settings.json.
export interface AskSettings {
  // Off by default: an ask sends the question off the machine.
  enabled: boolean;
  // Claude backend only.
  web_search: boolean;
  instructions: string;
  // null = the default Claude model.
  model: string | null;
  // null = low. Core turns "minimal" into "low" (the Claude CLI has
  // no such level).
  effort: RefineEffort | null;
  timeout_ms: number;
}

// Emitted after the Ask settings changed. Payload: `AskSettings`.
export const ASK_CHANGED_EVENT = "ask_changed";

// What `ask_get` returns (`AskState` in src-tauri/src/ask.rs): the
// stored block and what an ask would use right now. All of it is
// read off the settings, so the call is cheap. Whether the backend
// can run is not in it: that is `ask_status`.
export interface AskState {
  settings: AskSettings;
  // The backend Refine is set up with, which an ask uses too:
  // "claude-cli" or "openai-compatible".
  backend_kind: string;
  // Where questions are sent, as a part of a sentence.
  destination: string;
  model: string;
  effort: string;
  // Web search is on and the backend has one.
  web_search: boolean;
  timeout_ms: number;
  notice: string;
  limits: {
    timeout_min_ms: number;
    timeout_max_ms: number;
    timeout_default_ms: number;
    instructions_max_chars: number;
  };
  default_model: string;
}

// What `ask_status` returns: whether the backend an ask uses can run
// (`BackendStatus` in core, the one Refine's status is). Getting it
// starts the Claude CLI for a moment, so only the Ask pane asks.
export type AskBackendStatus = RefineBackendStatus;

// The ask is looking at the screen now, or a screenshot went out
// with the question: what the pill and the panel show.
export function askSawScreen(screen: AskScreen | undefined): boolean {
  return screen === "capturing" || screen === "included";
}

// An ask is listening, thinking or answering: a cancel would stop it.
export function askIsBusy(phase: AskPhase): boolean {
  return phase === "listening" || phase === "thinking" || phase === "answering";
}
