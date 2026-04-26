import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export const HOTKEY_STATUS_EVENT = "hotkey_status";

export interface HotkeyStatus {
  ok: boolean;
  error: string;
}

export interface HotkeyStatusView {
  status: HotkeyStatus | null;
  retry: () => Promise<void>;
}

export function useHotkeyStatus(): HotkeyStatusView {
  const [status, setStatus] = useState<HotkeyStatus | null>(null);

  useEffect(() => {
    // Boot install fires its event before the UI mounts; pull cached state
    // first so the banner renders correctly on first paint.
    void invoke<HotkeyStatus | null>("hotkey_status_current").then((s) => {
      if (s) setStatus(s);
    });

    const unlisten = listen<HotkeyStatus>(HOTKEY_STATUS_EVENT, (event) => {
      setStatus(event.payload);
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, []);

  const retry = useCallback(async () => {
    await invoke("hotkey_retry");
  }, []);

  return { status, retry };
}
