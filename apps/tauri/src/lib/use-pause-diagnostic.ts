import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// Mirror of `media_control::PauseDiagnostic` in Rust. `reason` is a
// stable machine tag the UI switches on — keep these strings in sync
// with the Rust mapping in `media_control/mod.rs::last_pause_diagnostic`.
export type PauseFailureReason = "not_authorized" | "no_known_player" | "other";

export interface PauseDiagnostic {
  reason: PauseFailureReason;
  detail: string;
}

// Subscribes to the most recent `pause_now` outcome. Backed by:
//   - initial pull via `media_get_last_pause_diagnostic` on mount
//   - live updates via `media_pause_diagnostic_changed` event emitted
//     after every recording-start in lib.rs::pause_audio_for_recording
//
// Why event-driven not polled: the diagnostic only changes on a
// recording boundary. A poll would either churn (frequent) or lag
// (infrequent). The event fires exactly when the value flips.
export function usePauseDiagnostic() {
  const [diagnostic, setDiagnostic] = useState<PauseDiagnostic | null>(null);

  useEffect(() => {
    invoke<PauseDiagnostic | null>("media_get_last_pause_diagnostic")
      .then((d) => setDiagnostic(d ?? null))
      .catch(() => setDiagnostic(null));
  }, []);

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    void listen<PauseDiagnostic | null>(
      "media_pause_diagnostic_changed",
      (event) => setDiagnostic(event.payload ?? null),
    ).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

  return diagnostic;
}
