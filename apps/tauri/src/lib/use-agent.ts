import { useCallback, useEffect } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import {
  AGENT_CHANGED_EVENT,
  choicesOf,
  withChoices,
  type AgentChoices,
  type AgentCustomConnection,
  type AgentRunningApp,
  type AgentState,
  type AgentStatus,
} from "./agent";
import { ASK_CHANGED_EVENT } from "./ask";

// Data for the Agent settings (TASK-99), on TanStack Query like Ask
// and Vision: Rust owns the Agent block and every rule about it, and
// the query cache is the one copy the webview keeps. A change is
// `agent_set` with the person's choices; Rust answers with the state
// as it stored it, and that answer is what ends up on screen.

const AGENT_KEY = ["agent"] as const;
// Whether an ask would use tools right now. Its own key: it follows
// from the Ask switch and the Refine backend as well.
const AGENT_STATUS_KEY = ["agent-status"] as const;
// The apps that are open now, for adding a row to the app table.
const AGENT_APPS_KEY = ["agent-apps"] as const;
// Every save, so one of them can tell whether another is under way.
const AGENT_SAVE_KEY = ["agent-save"] as const;

// The Agent state and ways to change it. `state` is null until the
// first read has answered. Reading starts nothing and calls no tool.
export function useAgent() {
  const client = useQueryClient();
  const query = useQuery({
    queryKey: AGENT_KEY,
    queryFn: () => invoke<AgentState>("agent_get"),
  });

  // The answer of a command is the new state. While another save is
  // still on its way, an earlier answer would take a later click back
  // off the screen for a moment, so only the last one is shown.
  const show = useCallback(
    (state: AgentState) => {
      if (client.isMutating({ mutationKey: AGENT_SAVE_KEY }) <= 1) {
        client.setQueryData<AgentState>(AGENT_KEY, state);
      }
      void client.invalidateQueries({ queryKey: AGENT_STATUS_KEY });
    },
    [client],
  );

  const save = useMutation({
    mutationKey: AGENT_SAVE_KEY,
    mutationFn: (choices: AgentChoices) => invoke<AgentState>("agent_set", { choices }),
    // Shown at once, so a switch or a tick does not lag behind the
    // click; undone if the save fails.
    onMutate: async (choices) => {
      await client.cancelQueries({ queryKey: AGENT_KEY });
      const previous = client.getQueryData<AgentState>(AGENT_KEY);
      if (previous) client.setQueryData<AgentState>(AGENT_KEY, withChoices(previous, choices));
      return { previous };
    },
    onError: (_error, _choices, context) => {
      if (context?.previous) client.setQueryData(AGENT_KEY, context.previous);
    },
    onSuccess: show,
  });

  // Ask Claude Code which connections it has. Takes a few seconds.
  // The answer is always a state: a lookup that failed says why in
  // `listing.error`.
  const lookUp = useMutation({
    mutationFn: () => invoke<AgentState>("agent_refresh"),
    onMutate: () => {
      const current = client.getQueryData<AgentState>(AGENT_KEY);
      if (current) {
        client.setQueryData<AgentState>(AGENT_KEY, {
          ...current,
          listing: { ...current.listing, refreshing: true },
        });
      }
    },
    onSuccess: show,
    onError: () => {
      void client.invalidateQueries({ queryKey: AGENT_KEY });
    },
  });

  const add = useMutation({
    mutationFn: (connection: AgentCustomConnection) =>
      invoke<AgentState>("agent_add_connection", { ...connection }),
    onSuccess: show,
  });

  const remove = useMutation({
    mutationFn: (name: string) => invoke<AgentState>("agent_remove_connection", { name }),
    onSuccess: show,
  });

  const { mutate } = save;
  // Change the choices as they are on screen now.
  const update = useCallback(
    (change: (choices: AgentChoices) => AgentChoices) => {
      const current = client.getQueryData<AgentState>(AGENT_KEY);
      if (current) mutate(change(choicesOf(current)));
    },
    [client, mutate],
  );

  const { mutate: startLookUp } = lookUp;
  const { mutateAsync: addAsync } = add;
  const { mutate: removeNow } = remove;

  const failure = query.error ?? save.error ?? lookUp.error ?? remove.error;
  return {
    state: query.data ?? null,
    error: failure ? String(failure) : null,
    update,
    refresh: useCallback(() => startLookUp(), [startLookUp]),
    // Resolves to what went wrong, or null when the connection was
    // added.
    addConnection: useCallback(
      (connection: AgentCustomConnection): Promise<string | null> =>
        addAsync(connection).then(
          () => null,
          (error) => String(error),
        ),
      [addAsync],
    ),
    removeConnection: useCallback((name: string) => removeNow(name), [removeNow]),
  };
}

// Whether an ask would use tools right now, and why not, in core's
// words (`agent_status`); null until it has answered. Asked again
// after every change of the Agent block and of the Ask block, and
// whenever the pane opens.
export function useAgentStatus(): AgentStatus | null {
  const query = useQuery({
    queryKey: AGENT_STATUS_KEY,
    queryFn: () => invoke<AgentStatus>("agent_status"),
  });
  return query.data ?? null;
}

// The apps that are open now (`agent_running_apps`), for adding one
// to the app table. Asked only while `enabled`, and again whenever
// the window gets focus back.
export function useAgentRunningApps(enabled: boolean): AgentRunningApp[] {
  const query = useQuery({
    queryKey: AGENT_APPS_KEY,
    queryFn: () => invoke<AgentRunningApp[]>("agent_running_apps"),
    enabled,
  });
  return query.data ?? [];
}

// Follow changes to the Agent state made anywhere: this window's own
// saves, the block edited by the headless CLI, and a lookup of the
// connections that ended (Rust sends `agent_changed` with the state).
// Whether an ask would use tools also follows the Ask switch, so that
// is asked again when the Ask block changes. Mounted once, in App.
export function useAgentSync() {
  const client = useQueryClient();
  useEffect(() => {
    const unlistenAgent = listen<AgentState>(AGENT_CHANGED_EVENT, (event) => {
      // A save of this window is still on its way: its answer is the
      // newer state.
      if (client.isMutating({ mutationKey: AGENT_SAVE_KEY }) === 0) {
        client.setQueryData<AgentState>(AGENT_KEY, event.payload);
      }
      void client.invalidateQueries({ queryKey: AGENT_STATUS_KEY });
    });
    const unlistenAsk = listen(ASK_CHANGED_EVENT, () => {
      void client.invalidateQueries({ queryKey: AGENT_STATUS_KEY });
    });
    return () => {
      void unlistenAgent.then((fn) => fn());
      void unlistenAsk.then((fn) => fn());
    };
  }, [client]);
}
