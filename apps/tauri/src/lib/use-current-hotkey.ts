import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  type HotkeyCapturedPayload,
  type HotkeyConfig,
  type HotkeySettings,
  type HotkeyTarget,
} from "./use-global-hotkey";

export function useCurrentHotkey(target: HotkeyTarget = "toggle"): HotkeyConfig | null {
  const [config, setConfig] = useState<HotkeyConfig | null>(null);

  useEffect(() => {
    let alive = true;
    void invoke<HotkeySettings>("settings_get_hotkeys").then((s) => {
      if (alive) setConfig(s[target]);
    });

    let unlisten: UnlistenFn | undefined;
    void listen<HotkeyCapturedPayload>("hotkey_captured", (evt) => {
      if (evt.payload.target === target) setConfig(evt.payload.config);
    }).then((fn) => {
      unlisten = fn;
    });

    return () => {
      alive = false;
      unlisten?.();
    };
  }, [target]);

  return config;
}
