import { useCallback, useEffect } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { ASK_CHANGED_EVENT, type AskSettings, type AskState } from "./ask";

// Data for the Ask settings (TASK-97), on TanStack Query like the
// history and the modes: Rust owns the Ask block and every rule
// about it, and the query cache is the one copy the webview keeps.
// A change is `ask_set`; Rust answers with the block as it stored it
// (trimmed, clamped), and that answer is what ends up on screen.

const ASK_KEY = ["ask"] as const;

// The Ask block, whether the backend can run and where questions are
// sent, and a way to change the block. `state` is null until the
// first read has answered.
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
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [client]);
}
