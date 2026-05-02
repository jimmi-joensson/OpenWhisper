import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// Persists in core settings (not localStorage) so the Rust-side phase
// observer in spawn_dictation_emitter can read it synchronously on every
// tick without invoking through the WebView. Default state is true to
// match the Rust-side default — otherwise the Switch starts unchecked
// for a frame before the first invoke resolves.
export function usePauseAudio() {
  const [enabled, setEnabledState] = useState(true);

  useEffect(() => {
    invoke<boolean>("behavior_get_pause_audio_during_dictation")
      .then(setEnabledState)
      .catch(() => setEnabledState(true));
  }, []);

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    void listen<boolean>("behavior_pause_audio_changed", (event) =>
      setEnabledState(event.payload),
    ).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

  const setEnabled = (next: boolean) =>
    invoke("behavior_set_pause_audio_during_dictation", { enabled: next });

  return { enabled, setEnabled };
}
