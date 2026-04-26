import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export const PERMISSIONS_STATUS_EVENT = "permissions_status";

export interface PermissionsStatus {
  mic_ok: boolean;
  mic_state: string;
  error: string;
}

export interface PermissionsStatusView {
  status: PermissionsStatus | null;
}

export function usePermissionsStatus(): PermissionsStatusView {
  const [status, setStatus] = useState<PermissionsStatus | null>(null);

  useEffect(() => {
    void invoke<PermissionsStatus | null>("permissions_status_current").then(
      (s) => {
        if (s) setStatus(s);
      },
    );

    const unlisten = listen<PermissionsStatus>(
      PERMISSIONS_STATUS_EVENT,
      (event) => {
        setStatus(event.payload);
      },
    );
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, []);

  return { status };
}
