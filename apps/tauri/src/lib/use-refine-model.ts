import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { MODES_CHANGED_EVENT } from "./modes";
import type { RefineModelInUse } from "./refine";
import { STATS_CHANGED_EVENT } from "./use-stats-summary";

// What a dictation would be refined with now, for the Refine card on
// Home: backend, model and effort of the selected mode. Asked again
// when the answer can have changed without Home being left: another
// mode is selected (it may have a model of its own), or a dictation
// finished (its run may be the first to name the exact model). The
// Refine settings themselves are changed on another screen, and Home
// asks again when it comes back. null until the first answer, and
// when the backend cannot say.
export function useRefineModel(): RefineModelInUse | null {
  const [model, setModel] = useState<RefineModelInUse | null>(null);

  useEffect(() => {
    let alive = true;
    const load = () => {
      invoke<RefineModelInUse | null>("refine_model_in_use")
        .then((m) => {
          if (alive) setModel(m ?? null);
        })
        .catch(() => {});
    };
    load();
    const unlisten = [listen(MODES_CHANGED_EVENT, load), listen(STATS_CHANGED_EVENT, load)];
    return () => {
      alive = false;
      for (const u of unlisten) void u.then((fn) => fn());
    };
  }, []);

  return model;
}
