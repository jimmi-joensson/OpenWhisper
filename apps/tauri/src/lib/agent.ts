// Types and event names for Agent (TASK-99): an ask can use the tool
// connections the person already has in their own Claude Code.
// Mirror src-tauri/src/agent.rs, core/src/settings/agent.rs and
// core/src/agent/.
//
// Rust owns every rule: which tool is of which kind, which tools an
// ask may use, whether an ask is an agent ask at all. What crosses
// into the webview is the person's choices and what to show for
// them. How a connection is reached (a command, a URL, a server's
// environment) never does.

import type { AskToolKind } from "./ask";

// Core's decision about a tool (`agent::policy`).
export type AgentToolKind = AskToolKind;

// Mirrors `ToolView` in src-tauri/src/agent.rs.
export interface AgentTool {
  // The tool's own name.
  name: string;
  // Name to show a person. May be "".
  title: string;
  kind: AgentToolKind;
  // Why the tool cannot be used at all, as core's sentence for the
  // person; null when it can. Such a tool is shown, never ticked.
  problem: string | null;
}

// Where a connection stood in the last lookup of this run of the
// app: one of core's `ConnectionState` words, "missing" (the lookup
// did not find it) or "not-looked-up" (there has been none yet, or
// it is an entry of the person's own that is switched off, which a
// lookup never starts).
export type AgentConnectionState =
  | "connected"
  | "needs-auth"
  | "failed"
  | "pending"
  | "disabled"
  | "unknown"
  | "missing"
  | "not-looked-up";

// Mirrors `ConnectionView` in src-tauri/src/agent.rs.
export interface AgentConnection {
  name: string;
  // "claude": one of the connections Claude Code has. "custom": the
  // person's own entry.
  kind: "claude" | "custom";
  enabled: boolean;
  // The ticked tools, by name.
  allowed_tools: string[];
  // The tools as they were last looked up.
  tools: AgentTool[];
  state: AgentConnectionState;
  // An ask can start with it right now. A connection of Claude Code
  // has to be looked up once after the app started.
  usable: boolean;
  // Only for the person's own entry: whether it is started with a
  // command or reached at a URL. Never the command or the URL.
  via: "command" | "url" | null;
}

// Mirrors `AppConnection` in core: an app, and the connection that
// reaches what is in it. `app_id` is the bundle id on macOS and the
// exe file name on Windows, in lower case.
export interface AgentAppConnection {
  app_id: string;
  // May be "".
  app_name: string;
  connection: string;
}

// Mirrors `AppRow` in src-tauri/src/agent.rs: a row of the app table
// in force, with what core makes of it.
export interface AgentAppRow extends AgentAppConnection {
  // The connection of that name, switched on or not; null when
  // there is none.
  known: string | null;
  // An ask in this app would use the connection instead of the
  // screen right now.
  active: boolean;
}

// Mirrors `ListingView` in src-tauri/src/agent.rs.
export interface AgentListing {
  // A lookup is running now.
  refreshing: boolean;
  // When the last lookup that worked ended; null before the first.
  refreshed_at_ms: number | null;
  // Why the last lookup failed; "" when it worked.
  error: string;
  // Why the person's own entries could not be looked up; "" when
  // they were, or when there are none.
  custom_error: string;
}

// What `agent_get` returns (`AgentState` in src-tauri/src/agent.rs),
// and the payload of `agent_changed`.
export interface AgentState {
  enabled: boolean;
  drafts_only: boolean;
  // How long a confirm waits before it counts as a no.
  confirm_timeout_s: number;
  max_turns: number;
  connections: AgentConnection[];
  // The app table in force: the person's, or the built-in one.
  app_connections: AgentAppRow[];
  app_connections_builtin: boolean;
  listing: AgentListing;
}

// What `agent_set` takes (`AgentPatch` in src-tauri/src/agent.rs):
// one change. Only what is named is changed, and Rust lays it over
// the block as it is on disk at that moment, so a change made
// elsewhere in the meantime (the headless CLI) is kept. A switch
// that is left out is not changed: the only way to switch drafts
// only off is `drafts_only: false`. A connection that is not there
// cannot be added this way, and what a connection is never changes
// with it.
export interface AgentPatch {
  enabled?: boolean;
  drafts_only?: boolean;
  // One connection: its switch, and tools to tick or to untick. Its
  // other ticks stay as they are.
  connection?: { name: string; enabled?: boolean; tick?: string[]; untick?: string[] };
  // The whole app table; null = back to the built-in one.
  app_connections?: AgentAppConnection[] | null;
}

// What `agent_status` returns: whether an ask would use tools right
// now, and why not, in core's words.
export interface AgentStatus {
  ready: boolean;
  message: string;
  ask_enabled: boolean;
  // "claude-cli" or "openai-compatible".
  backend: string;
}

// What `agent_running_apps` returns: an app that is open now.
export interface AgentRunningApp {
  app_id: string;
  app_name: string;
}

// Emitted after the Agent settings changed or a lookup ended.
// Payload: `AgentState`.
export const AGENT_CHANGED_EVENT = "agent_changed";

// The three kinds of tool, in the order they are shown, with what
// each one means for the person.
export const AGENT_TOOL_KINDS: { kind: AgentToolKind; label: string; description: string }[] = [
  {
    kind: "read",
    label: "Read",
    description: "Look something up. They change nothing and run without asking.",
  },
  {
    kind: "draft",
    label: "Draft",
    description: "Prepare a draft you can still throw away. They run without asking.",
  },
  {
    kind: "act",
    label: "Act",
    description: "Send, post, delete or change something, and every tool that cannot be told.",
  },
];

// What the state of a connection is called.
export const AGENT_STATE_LABELS: Record<AgentConnectionState, string> = {
  connected: "Connected",
  "needs-auth": "Needs sign-in",
  failed: "Failed",
  pending: "Still connecting",
  disabled: "Off in Claude Code",
  unknown: "State unknown",
  missing: "Not found",
  "not-looked-up": "Not looked up yet",
};

// What to call a tool: its title, or its name when it has none.
export function toolLabel(tool: AgentTool): string {
  return tool.title.trim() || tool.name;
}

// What to call an app: its name, or its id when it has none.
export function agentAppLabel(app: { app_id: string; app_name: string }): string {
  return app.app_name.trim() || app.app_id;
}

// A state with a change shown at once, before Rust has answered with
// what it stored: the switches and the ticks. The app table waits for
// the answer, since what a row leads to is core's to say.
export function withPatch(state: AgentState, patch: AgentPatch): AgentState {
  const change = patch.connection;
  return {
    ...state,
    enabled: patch.enabled ?? state.enabled,
    drafts_only: patch.drafts_only ?? state.drafts_only,
    connections: state.connections.map((connection) => {
      if (!change || change.name !== connection.name) return connection;
      const untick = change.untick ?? [];
      const kept = connection.allowed_tools.filter((t) => !untick.includes(t));
      const added = (change.tick ?? []).filter((t) => !kept.includes(t));
      return {
        ...connection,
        enabled: change.enabled ?? connection.enabled,
        allowed_tools: [...kept, ...added],
      };
    }),
  };
}
