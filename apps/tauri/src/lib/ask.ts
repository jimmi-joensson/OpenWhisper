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
  // A tool call waits for the person's yes or no (Agent, TASK-99).
  | "confirming"
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

// Mirrors `AskSuggestion` in core: something the person could do to
// get a better answer next time. The model decides when it applies;
// the panel says it in the app's words. "web-search": the question
// needed current information from the web and web search is off.
export type AskSuggestion = "web-search";

// Mirrors `ToolKind` in core/src/ask/steps.rs: what a tool does, as
// far as core can tell (Agent, TASK-99). "act" is everything that
// sends, posts, deletes or changes something, and anything unknown.
export type AskToolKind = "read" | "draft" | "act";

// Mirrors `StepState` in core: where a tool call stands.
export type AskStepState =
  // The tool is running.
  | "running"
  // It ran and answered.
  | "done"
  // It ran and reported an error.
  | "failed"
  // It did not run: not allowed, the person said no or did not
  // answer in time, or Claude Code stopped while it waited.
  | "denied"
  // It waits for the person's yes or no.
  | "waiting";

// Mirrors `AskStep` in core: one tool call of an ask.
export interface AskStep {
  // Tells one call from the next within an ask.
  id: string;
  // Name of the connection the tool belongs to.
  connection: string;
  // The tool's own name.
  tool: string;
  // Name to show a person; the tool's name when it has no other.
  title: string;
  kind: AskToolKind;
  // The input on one line, cut short by core.
  input_summary: string;
  state: AskStepState;
  // What came of it, on one line. "" while it runs or waits.
  outcome: string;
}

// Mirrors `AskConfirm` in core: a tool call that waits for the
// person's yes or no. Answered with `ask_confirm_allow` or
// `ask_confirm_deny`, naming the ask and this request.
export interface AskConfirm {
  request_id: string;
  // The step this is about (`AskStep.id`).
  step_id: string;
  connection: string;
  tool: string;
  title: string;
  // The whole input as indented JSON text: never cut (core refuses a
  // call whose input is too long to show, without asking). Short
  // fields come first, and characters that would not show as what
  // they are arrive as visible escapes.
  input: string;
  // How long the confirm waits before it counts as a no.
  timeout_s: number;
  // How long until a yes to this confirm counts, in milliseconds:
  // 600 when it is raised, counting down to 0. Until then core
  // refuses a yes and the confirm stays open, so Allow shows as not
  // yet active. A no is taken at any time.
  armed_in_ms: number;
}

// Mirrors `ContextSource` in core: where an ask looked for what the
// question is about. "connection": the app in front has a connection
// and the ask was pointed at that instead of the screen.
export type AskContextSource =
  | { kind: "none" }
  | { kind: "screen" }
  | { kind: "connection"; name: string };

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
  // Set as soon as the model marked its answer, so it can show while
  // the answer streams. null when there is nothing to suggest.
  suggestion: AskSuggestion | null;
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
  // Agent (TASK-99): every tool call of this ask, in the order they
  // were made. Empty for an ask without tools.
  steps: AskStep[];
  // The tool call that waits for the person; null when none does.
  // Set exactly while the phase is "confirming".
  confirm: AskConfirm | null;
  // Where the ask looked for its context.
  context_source: AskContextSource;
  // Goes up on every change.
  version: number;
}

// Emitted when the ask changes, and about four times a second while
// one is in flight. Payload: `AskSnapshot`.
export const ASK_TICK_EVENT = "ask_tick";

// Mirrors `ContextSettings` in core: what an ask tells the model
// about this computer. Four groups of facts the app reads by itself,
// each with its own switch, and one line the person types.
export interface AskContextSettings {
  // The date, the local time and the time zone.
  date_time: boolean;
  // System, model, chip, memory, power, displays, free disk space,
  // online or offline.
  computer: boolean;
  // The name of the app in front. Never the title of its window.
  front_app: boolean;
  // Language, region, units, clock, currency, keyboard layout.
  locale: boolean;
  // Where the person is, in their own words. "" = not given.
  place: string;
}

// `ContextSettings::default()` in core: every group on, no place.
export const DEFAULT_ASK_CONTEXT: AskContextSettings = {
  date_time: true,
  computer: true,
  front_app: true,
  locale: true,
  place: "",
};

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
  // What an ask tells the model about this computer.
  context: AskContextSettings;
}

// The context block of `settings`. A block that came without one (a
// shell from before the context existed, while the interface is
// being worked on) reads as the defaults, like a settings file from
// before it does in core.
export function askContext(settings: AskSettings | null | undefined): AskContextSettings {
  return { ...DEFAULT_ASK_CONTEXT, ...(settings?.context ?? {}) };
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
    place_max_chars: number;
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

// An ask is listening, thinking, answering or waiting on a confirm:
// a cancel would stop it (`AskPhase::is_busy` in core). A cancel
// while a confirm waits is a no to it.
export function askIsBusy(phase: AskPhase): boolean {
  return (
    phase === "listening" ||
    phase === "thinking" ||
    phase === "answering" ||
    phase === "confirming"
  );
}
