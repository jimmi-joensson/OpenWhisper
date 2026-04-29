import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// Persists in core settings (not localStorage) so the Rust-side fullscreen
// detector callback can read the boolean synchronously without invoking
// through the WebView. The Switch in General → Behavior subscribes via
// invoke for the initial read + listen for cross-surface updates emitted
// by `behavior_set_show_in_fullscreen`.
export function useShowInFullscreen() {
  const [enabled, setEnabledState] = useState(false);

  useEffect(() => {
    invoke<boolean>("behavior_get_show_in_fullscreen")
      .then(setEnabledState)
      .catch(() => setEnabledState(false));
  }, []);

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    void listen<boolean>("behavior_show_in_fullscreen_changed", (event) =>
      setEnabledState(event.payload),
    ).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

  const setEnabled = (next: boolean) =>
    invoke("behavior_set_show_in_fullscreen", { enabled: next });

  return { enabled, setEnabled };
}
