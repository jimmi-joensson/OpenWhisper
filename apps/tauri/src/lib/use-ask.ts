import { useCallback, useEffect } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import {
  ASK_CHANGED_EVENT,
  type AskBackendStatus,
  type AskSettings,
  type AskState,
} from "./ask";

// Data for the Ask settings (TASK-97), on TanStack Query like the
// history and the modes: Rust owns the Ask block and every rule
// about it, and the query cache is the one copy the webview keeps.
// A change is `ask_set`; Rust answers with the block as it stored it
// (trimmed, clamped), and that answer is what ends up on screen.

const ASK_KEY = ["ask"] as const;
// Its own key, not under `ASK_KEY`: a change of the Ask block does
// not change whether the backend can run, so it must not start the
// check again.
const ASK_BACKEND_KEY = ["ask-backend"] as const;
// The preview of the device context. Its own key as well: the text
// is read from the computer, not stored, and is asked for again after
// every save and whenever the person wants a fresh one.
const ASK_CONTEXT_KEY = ["ask-context"] as const;

// The Ask block, what an ask would use right now and where questions
// are sent, and a way to change the block. `state` is null until the
// first read has answered. This is the one way the webview reads the
// Ask settings: every caller shares the cache entry, and
// `useAskSync` keeps it in step with Rust. The read is cheap
// (`ask_get` asks nothing of the backend).
export function useAsk() {
  const client = useQueryClient();
  const query = useQuery({
    queryKey: ASK_KEY,
    queryFn: () => invoke<AskState>("ask_get"),
  });

  const save = useMutation({
    mutationFn: (settings: AskSettings) => invoke<AskSettings>("ask_set", { settings }),
    // Shown at once, so a switch does not lag behind the click;
    // undone if the save fails.
    onMutate: async (settings) => {
      await client.cancelQueries({ queryKey: ASK_KEY });
      const previous = client.getQueryData<AskState>(ASK_KEY);
      if (previous) client.setQueryData<AskState>(ASK_KEY, { ...previous, settings });
      return { previous };
    },
    onError: (_error, _settings, context) => {
      if (context?.previous) client.setQueryData(ASK_KEY, context.previous);
    },
    onSuccess: (saved) => {
      const current = client.getQueryData<AskState>(ASK_KEY);
      if (current) client.setQueryData<AskState>(ASK_KEY, { ...current, settings: saved });
      // Rust has the new block now: what it would tell the model
      // may have changed with it.
      void client.invalidateQueries({ queryKey: ASK_CONTEXT_KEY });
      // The rest of the state (what an ask would use right now)
      // follows from the block: `useAskSync` re-reads it when Rust
      // announces the change.
    },
  });

  const { mutate } = save;
  const update = useCallback(
    (patch: Partial<AskSettings>) => {
      const current = client.getQueryData<AskState>(ASK_KEY);
      if (current) mutate({ ...current.settings, ...patch });
    },
    [client, mutate],
  );

  const failure = query.error ?? save.error;
  return {
    state: query.data ?? null,
    error: failure ? String(failure) : null,
    update,
  };
}

// Whether the backend an ask uses can run (`ask_status`); null until
// the check has answered. The check starts the Claude CLI for a
// moment, so this is for the Ask pane alone, which shows the result.
// It runs when the pane opens and when the window gets focus again.
export function useAskBackend(): AskBackendStatus | null {
  const query = useQuery({
    queryKey: ASK_BACKEND_KEY,
    queryFn: () => invoke<AskBackendStatus>("ask_status"),
  });
  return query.data ?? null;
}

// The facts about this computer an ask would send along if it
// started now, as the exact text the model would get
// (`ask_context_preview`; the same text `openwhisper ask context`
// prints). Rust reads them off the stored Ask block, so the text is
// asked for again after every save of the block (see `useAsk` and
// `useAskSync`), and `refresh` asks again on demand, since the time
// in it moves. `text` is null until the first read has answered;
// "" means there is nothing to tell. Nothing is sent anywhere.
export function useAskContextPreview() {
  const query = useQuery({
    queryKey: ASK_CONTEXT_KEY,
    queryFn: () => invoke<string>("ask_context_preview"),
  });
  const { refetch } = query;
  const refresh = useCallback(() => {
    void refetch();
  }, [refetch]);
  return {
    text: query.data ?? null,
    error: query.error ? String(query.error) : null,
    refreshing: query.isFetching,
    refresh,
  };
}

// Follow changes to the Ask block made anywhere: this window's own
// saves, and the block edited by the headless CLI (Rust sends
// `ask_changed` with the stored block after every change). The block
// is shown at once and the whole state is read again, since what an
// ask would use follows from it. Mounted once, in App.
export function useAskSync() {
  const client = useQueryClient();
  useEffect(() => {
    const unlisten = listen<AskSettings>(ASK_CHANGED_EVENT, (event) => {
      const current = client.getQueryData<AskState>(ASK_KEY);
      if (current) {
        client.setQueryData<AskState>(ASK_KEY, { ...current, settings: event.payload });
      }
      void client.invalidateQueries({ queryKey: ASK_KEY });
      void client.invalidateQueries({ queryKey: ASK_CONTEXT_KEY });
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [client]);
}
