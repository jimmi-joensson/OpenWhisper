import { useCallback, useEffect } from "react";
import {
  useMutation,
  useQuery,
  useQueryClient,
  type QueryClient,
} from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import {
  MODES_CHANGED_EVENT,
  type ModeApp,
  type ModeDraft,
  type ModesChanged,
  type ModesNow,
  type ModesState,
} from "./modes";
import type { RefineState } from "./refine";

// Data for modes (TASK-96), on TanStack Query like the history: Rust
// owns the modes block and every rule about it, the query cache is
// the one copy the webview keeps, and every screen that shows modes
// (the select on Home, the Modes pane) reads from it. A change is a
// command; Rust answers with the modes as it stored them, and that
// answer is what ends up on screen.

const MODES_KEY = ["modes"] as const;
const REFINE_KEY = ["refine", "state"] as const;

// Put Rust's answer in the cache. The list of apps is not part of a
// change's answer, so it stays as it was read.
function store(client: QueryClient, now: ModesNow) {
  const previous = client.getQueryData<ModesState>(MODES_KEY);
  if (previous) {
    client.setQueryData<ModesState>(MODES_KEY, {
      ...previous,
      settings: now.settings,
      modes: now.modes,
    });
  } else {
    void client.invalidateQueries({ queryKey: MODES_KEY });
  }
}

// One change to the modes. `optimistic` shows the change before Rust
// has answered, for the controls where a pause would be seen (a
// select snapping back and forth); it is undone if the command fails
// and replaced by Rust's answer when it succeeds. Changes whose
// outcome only core knows (the id of a new mode, what a delete takes
// with it) have none and show Rust's answer.
function useChange<Args>(
  run: (args: Args) => Promise<ModesNow>,
  optimistic?: (state: ModesState, args: Args) => ModesState,
) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: run,
    onMutate: async (args: Args) => {
      if (!optimistic) return { previous: undefined };
      await client.cancelQueries({ queryKey: MODES_KEY });
      const previous = client.getQueryData<ModesState>(MODES_KEY);
      if (previous) {
        client.setQueryData<ModesState>(MODES_KEY, optimistic(previous, args));
      }
      return { previous };
    },
    onError: (_error, _args, context) => {
      if (context?.previous) client.setQueryData(MODES_KEY, context.previous);
    },
    onSuccess: (now) => store(client, now),
  });
}

interface AppDefaultArgs {
  app: ModeApp;
  modeId: string;
}

// The modes, the selected one, the apps that can have a default, and
// the changes. `state` is null until the first read has answered.
export function useModes() {
  const query = useQuery({
    queryKey: MODES_KEY,
    queryFn: () => invoke<ModesState>("modes_get"),
  });

  const select = useChange(
    (id: string) => invoke<ModesNow>("modes_select", { id }),
    (state, id) => ({ ...state, settings: { ...state.settings, active: id } }),
  );
  const add = useChange((draft: ModeDraft) => invoke<ModesNow>("modes_add", { ...draft }));
  const edit = useChange(({ id, draft }: { id: string; draft: ModeDraft }) =>
    invoke<ModesNow>("modes_edit", { id, ...draft }),
  );
  const remove = useChange((id: string) => invoke<ModesNow>("modes_delete", { id }));
  const setAppDefault = useChange(
    ({ app, modeId }: AppDefaultArgs) =>
      invoke<ModesNow>("modes_set_app_default", {
        appId: app.id,
        appName: app.name,
        modeId,
      }),
    (state, { app, modeId }) => ({
      ...state,
      settings: {
        ...state.settings,
        app_defaults: [
          ...state.settings.app_defaults.filter((d) => d.app_id !== app.id),
          { app_id: app.id, app_name: app.name, mode: modeId },
        ],
      },
    }),
  );
  const clearAppDefault = useChange(
    (appId: string) => invoke<ModesNow>("modes_clear_app_default", { appId }),
    (state, appId) => ({
      ...state,
      settings: {
        ...state.settings,
        app_defaults: state.settings.app_defaults.filter((d) => d.app_id !== appId),
      },
    }),
  );

  const { mutateAsync: addAsync } = add;
  const { mutateAsync: editAsync } = edit;
  // Add a mode (`id` null) or change one. Resolves to what went wrong,
  // or null when it is saved, so the dialog knows whether to close.
  const save = useCallback(
    async (id: string | null, draft: ModeDraft): Promise<string | null> => {
      try {
        if (id === null) await addAsync(draft);
        else await editAsync({ id, draft });
        return null;
      } catch (e) {
        return String(e);
      }
    },
    [addAsync, editAsync],
  );

  const failure =
    query.error ??
    select.error ??
    remove.error ??
    setAppDefault.error ??
    clearAppDefault.error;

  return {
    state: query.data ?? null,
    error: failure ? String(failure) : null,
    select: select.mutate,
    save,
    remove: remove.mutate,
    setAppDefault: setAppDefault.mutate,
    clearAppDefault: clearAppDefault.mutate,
  };
}

// Follow changes made outside this window's own commands: the
// switch-mode hotkey and the tray menu. Rust sends the modes as they
// are now with every change. Mounted once, in App.
export function useModesSync() {
  const client = useQueryClient();
  useEffect(() => {
    const unlisten = listen<ModesChanged>(MODES_CHANGED_EVENT, (event) => {
      store(client, event.payload);
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [client]);
}

// The Refine settings, read only, for the screens that explain or
// offer something that depends on them: modes apply only while Refine
// is on, and a mode's own model is a model of the Refine backend.
// Changing them is the Refine pane's job.
export function useRefineState() {
  const query = useQuery({
    queryKey: REFINE_KEY,
    queryFn: () => invoke<RefineState>("refine_get"),
  });
  return query.data ?? null;
}
