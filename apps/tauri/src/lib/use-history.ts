import { useCallback, useEffect, useState } from "react";
import {
  keepPreviousData,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import type { HistoryPage, HistorySettings, HistoryState } from "./history";

// Data for transcript history (TASK-95), on TanStack Query: Rust owns
// the values, the query cache is the one copy the webview keeps, and
// every screen that shows history reads from it. So the switch on
// Home and the one in Settings cannot disagree, and a list re-reads
// itself when its inputs or the store change.

const SETTINGS_KEY = ["history", "settings"] as const;
const LISTS_KEY = ["history", "list"] as const;

// The history switch and retention, for Home and Settings → General.
// Changes show at once and snap to what Rust stored (it clamps).
export function useHistorySettings() {
  const client = useQueryClient();
  const query = useQuery({
    queryKey: SETTINGS_KEY,
    queryFn: () => invoke<HistoryState>("history_get"),
  });

  const save = useMutation({
    mutationFn: (settings: HistorySettings) =>
      invoke<HistorySettings>("history_set", { settings }),
    onMutate: async (settings) => {
      await client.cancelQueries({ queryKey: SETTINGS_KEY });
      const previous = client.getQueryData<HistoryState>(SETTINGS_KEY);
      if (previous) {
        client.setQueryData<HistoryState>(SETTINGS_KEY, { ...previous, settings });
      }
      return { previous };
    },
    onError: (_error, _settings, context) => {
      if (context?.previous) client.setQueryData(SETTINGS_KEY, context.previous);
    },
    onSuccess: (saved) => {
      client.setQueryData<HistoryState>(SETTINGS_KEY, (state) =>
        state ? { ...state, settings: saved } : state,
      );
      // A shorter retention removes stored text.
      void client.invalidateQueries({ queryKey: LISTS_KEY });
    },
  });

  const { mutate } = save;
  const update = useCallback(
    (patch: Partial<HistorySettings>) => {
      const current = client.getQueryData<HistoryState>(SETTINGS_KEY);
      if (current) mutate({ ...current.settings, ...patch });
    },
    [client, mutate],
  );

  const failure = query.error ?? save.error;
  return {
    settings: query.data?.settings ?? null,
    database: query.data?.database ?? "",
    error: failure ? String(failure) : null,
    update,
  };
}

// One page of stored dictations, newest first. `page` counts from 0.
// While the next page or search loads, the previous answer stays on
// screen instead of the list blinking empty.
export function useHistoryPage(search: string, page: number, pageSize: number) {
  const query = useQuery({
    queryKey: [...LISTS_KEY, { search, page, pageSize }],
    queryFn: () =>
      invoke<HistoryPage>("history_list", {
        search,
        limit: pageSize,
        offset: page * pageSize,
      }),
    placeholderData: keepPreviousData,
  });
  return {
    page: query.data ?? null,
    // The data on screen belongs to an earlier search or page.
    stale: query.isPlaceholderData,
    error: query.error ? String(query.error) : null,
  };
}

// Remove one entry, or everything. Every list re-reads afterwards.
export function useHistoryActions() {
  const client = useQueryClient();
  const refresh = () => client.invalidateQueries({ queryKey: LISTS_KEY });
  const remove = useMutation({
    mutationFn: (id: number) => invoke<boolean>("history_delete", { id }),
    onSettled: refresh,
  });
  const clear = useMutation({
    mutationFn: () => invoke<number>("history_clear"),
    onSettled: refresh,
  });
  const failure = remove.error ?? clear.error;
  return {
    remove: remove.mutate,
    clear: clear.mutate,
    error: failure ? String(failure) : null,
  };
}

// Re-read the lists when Rust says the store changed: every finished
// dictation, and every removal made elsewhere (the CLI, retention).
// Mounted once, in App.
export function useHistorySync() {
  const client = useQueryClient();
  useEffect(() => {
    const unlisten = listen("stats_changed", () => {
      void client.invalidateQueries({ queryKey: LISTS_KEY });
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [client]);
}

// `value`, but only after it has stopped changing for `delayMs`. For
// the search field: the list follows the typing with a short pause.
export function useDebounced<T>(value: T, delayMs: number): T {
  const [settled, setSettled] = useState(value);
  useEffect(() => {
    const timer = setTimeout(() => setSettled(value), delayMs);
    return () => clearTimeout(timer);
  }, [value, delayMs]);
  return settled;
}
