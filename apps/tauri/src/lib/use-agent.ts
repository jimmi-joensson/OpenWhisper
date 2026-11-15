import { useCallback, useEffect } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import {
  AGENT_CHANGED_EVENT,
  withPatch,
  type AgentPatch,
  type AgentRunningApp,
  type AgentState,
  type AgentStatus,
} from "./agent";
import { ASK_CHANGED_EVENT } from "./ask";

// Data for the Agent settings (TASK-99), on TanStack Query like Ask
// and Vision: Rust owns the Agent block and every rule about it, and
// the query cache is the one copy the webview keeps. A change is
// `agent_set` with that one change and nothing else; Rust lays it
// over the block as it is on disk and answers with the state as it
// stored it, and that answer is what ends up on screen.

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

  // A state Rust answered with goes on screen, unless a save is
  // still on its way: a click that is shown already (a tick, a
  // switch) would be taken back off the screen for a moment by a
  // state from before it. The answer of the last save puts things
  // right. `own`: the state is the answer of a save itself, which is
  // still counted as on its way while its answer is handled.
  const show = useCallback(
    (state: AgentState, own: boolean) => {
      if (client.isMutating({ mutationKey: AGENT_SAVE_KEY }) <= (own ? 1 : 0)) {
        client.setQueryData<AgentState>(AGENT_KEY, state);
      }
      void client.invalidateQueries({ queryKey: AGENT_STATUS_KEY });
    },
    [client],
  );

  // Read the block again, as it is on disk now: it can have been
  // changed elsewhere (the headless CLI) without this window being
  // told. Not while a save is on its way: its answer is that read.
  const reread = useCallback(() => {
    if (client.isMutating({ mutationKey: AGENT_SAVE_KEY }) > 0) return;
    void client.invalidateQueries({ queryKey: AGENT_KEY });
    void client.invalidateQueries({ queryKey: AGENT_STATUS_KEY });
  }, [client]);

  // Whenever the window gets the focus back. TanStack Query reads
  // again when a page becomes visible, which a window that was only
  // behind another one never stopped being.
  useEffect(() => {
    window.addEventListener("focus", reread);
    return () => window.removeEventListener("focus", reread);
  }, [reread]);

  const save = useMutation({
    mutationKey: AGENT_SAVE_KEY,
    mutationFn: (patch: AgentPatch) => invoke<AgentState>("agent_set", { patch }),
    // Shown at once, so a switch or a tick does not lag behind the
    // click; undone if the save fails.
    onMutate: async (patch) => {
      await client.cancelQueries({ queryKey: AGENT_KEY });
      const previous = client.getQueryData<AgentState>(AGENT_KEY);
      if (previous) client.setQueryData<AgentState>(AGENT_KEY, withPatch(previous, patch));
      return { previous };
    },
    onError: (_error, _patch, context) => {
      if (context?.previous) client.setQueryData(AGENT_KEY, context.previous);
    },
    // The answer is the block as Rust read it from disk and stored
    // it with this change.
    onSuccess: (state) => show(state, true),
    // After every save, taken or refused, the block is read again:
    // what is on disk is what the pane shows, also when that is not
    // what this window thought it was. Not while a later save is on
    // its way (this one is still counted while it is settled here).
    onSettled: () => {
      if (client.isMutating({ mutationKey: AGENT_SAVE_KEY }) <= 1) {
        void client.invalidateQueries({ queryKey: AGENT_KEY });
      }
    },
  });

  // Ask Claude Code which connections it has. Takes a few seconds.
  // The answer is a state: a lookup that ran and failed says why in
  // `listing.error`. One that Rust did not start (Agent, Ask or the
  // Claude backend is missing) is an error that says which.
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
    onSuccess: (state) => show(state, false),
    onError: () => {
      void client.invalidateQueries({ queryKey: AGENT_KEY });
    },
  });

  const remove = useMutation({
    mutationFn: (name: string) => invoke<AgentState>("agent_remove_connection", { name }),
    onSuccess: (state) => show(state, false),
    onError: () => {
      void client.invalidateQueries({ queryKey: AGENT_KEY });
    },
  });

  const { mutate } = save;
  const { mutate: startLookUp } = lookUp;
  const { mutate: removeNow } = remove;

  const failure = query.error ?? save.error ?? lookUp.error ?? remove.error;
  return {
    state: query.data ?? null,
    error: failure ? String(failure) : null,
    // One change: a switch, one connection's switch or ticks, or
    // the app table.
    update: useCallback((patch: AgentPatch) => mutate(patch), [mutate]),
    refresh: useCallback(() => startLookUp(), [startLookUp]),
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
  const client = useQueryClient();
  const query = useQuery({
    queryKey: AGENT_APPS_KEY,
    queryFn: () => invoke<AgentRunningApp[]>("agent_running_apps"),
    enabled,
  });
  useEffect(() => {
    const again = () => void client.invalidateQueries({ queryKey: AGENT_APPS_KEY });
    window.addEventListener("focus", again);
    return () => window.removeEventListener("focus", again);
  }, [client]);
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
