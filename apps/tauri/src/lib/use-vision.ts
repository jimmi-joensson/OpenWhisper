import { useCallback, useEffect } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import {
  VISION_CHANGED_EVENT,
  withSettings,
  type NeverCaptureApp,
  type VisionPermission,
  type VisionSettings,
  type VisionState,
} from "./vision";

// Data for the Vision settings (TASK-98), on TanStack Query like Ask,
// the history and the modes: Rust owns the Vision block and every
// rule about it, and the query cache is the one copy the webview
// keeps. A change is `vision_set`; Rust answers with the block as it
// stored it (lists tidied), and that answer is what ends up on screen.

const VISION_KEY = ["vision"] as const;
// The apps that are open now, for the "add app" picker. Its own key:
// a change of the block does not change which apps are open.
const VISION_APPS_KEY = ["vision-apps"] as const;

// The Vision block, the lists in force, the permission state, and
// ways to change them. `state` is null until the first read has
// answered. Reading takes no screenshot and asks for no permission.
export function useVision() {
  const client = useQueryClient();
  const query = useQuery({
    queryKey: VISION_KEY,
    queryFn: () => invoke<VisionState>("vision_get"),
  });

  const save = useMutation({
    mutationFn: (settings: VisionSettings) =>
      invoke<VisionSettings>("vision_set", { settings }),
    // Shown at once, so the select and the lists do not lag behind
    // the click; undone if the save fails.
    onMutate: async (settings) => {
      await client.cancelQueries({ queryKey: VISION_KEY });
      const previous = client.getQueryData<VisionState>(VISION_KEY);
      if (previous) client.setQueryData<VisionState>(VISION_KEY, withSettings(previous, settings));
      return { previous };
    },
    onError: (_error, _settings, context) => {
      if (context?.previous) client.setQueryData(VISION_KEY, context.previous);
    },
    onSuccess: (saved) => {
      const current = client.getQueryData<VisionState>(VISION_KEY);
      if (current) client.setQueryData<VisionState>(VISION_KEY, withSettings(current, saved));
    },
  });

  // The system's answer to a request is the new state. A grant given
  // in System Settings only counts after a restart, so this can stay
  // "denied"; the section says so.
  const request = useMutation({
    mutationFn: () => invoke<VisionPermission>("vision_request_permission"),
    onSuccess: (permission) => {
      const current = client.getQueryData<VisionState>(VISION_KEY);
      if (current) client.setQueryData<VisionState>(VISION_KEY, { ...current, permission });
    },
  });

  const { mutate } = save;
  const update = useCallback(
    (patch: Partial<VisionSettings>) => {
      const current = client.getQueryData<VisionState>(VISION_KEY);
      if (current) mutate({ ...current.settings, ...patch });
    },
    [client, mutate],
  );

  const { mutate: requestPermission } = request;
  const openSystemSettings = useCallback(() => {
    void invoke<boolean>("vision_open_settings").catch(() => {});
  }, []);

  const failure = query.error ?? save.error ?? request.error;
  return {
    state: query.data ?? null,
    error: failure ? String(failure) : null,
    update,
    requestPermission: useCallback(() => requestPermission(), [requestPermission]),
    requesting: request.isPending,
    openSystemSettings,
  };
}

// The apps that are open now (`vision_running_apps`), for adding one
// to the never-capture list. Asked only while `enabled` (the picker
// is shown), and again whenever the window gets focus back, since
// the person may have just opened the app they want to add.
export function useVisionRunningApps(enabled: boolean): NeverCaptureApp[] {
  const query = useQuery({
    queryKey: VISION_APPS_KEY,
    queryFn: () => invoke<NeverCaptureApp[]>("vision_running_apps"),
    enabled,
  });
  return query.data ?? [];
}

// Follow changes to the Vision block made anywhere: this window's own
// saves, and the block edited by the headless CLI (Rust sends
// `vision_changed` with the stored block after every change). The
// block is shown at once and the whole state is read again, since the
// permission may have changed too. Mounted once, in App.
export function useVisionSync() {
  const client = useQueryClient();
  useEffect(() => {
    const unlisten = listen<VisionSettings>(VISION_CHANGED_EVENT, (event) => {
      const current = client.getQueryData<VisionState>(VISION_KEY);
      if (current) {
        client.setQueryData<VisionState>(VISION_KEY, withSettings(current, event.payload));
      }
      void client.invalidateQueries({ queryKey: VISION_KEY });
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [client]);
}
