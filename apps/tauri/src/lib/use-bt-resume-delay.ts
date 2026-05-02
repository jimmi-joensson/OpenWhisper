import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// Persists in core settings (BehaviorSettings.bt_resume_delay_ms) and
// reads through an AtomicU64 cache on the Rust side. The Windows
// MediaController's resume_now reads it once per recording end. Default
// state is 5000 to match the Rust-side schema default — otherwise the
// Select starts blank for a frame before the first invoke resolves.
export function useBtResumeDelay() {
  const [delayMs, setDelayMsState] = useState(5000);

  useEffect(() => {
    invoke<number>("behavior_get_bt_resume_delay_ms")
      .then(setDelayMsState)
      .catch(() => setDelayMsState(5000));
  }, []);

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    void listen<number>("behavior_bt_resume_delay_changed", (event) =>
      setDelayMsState(event.payload),
    ).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

  const setDelayMs = (next: number) =>
    invoke("behavior_set_bt_resume_delay_ms", { delayMs: next });

  return { delayMs, setDelayMs };
}
