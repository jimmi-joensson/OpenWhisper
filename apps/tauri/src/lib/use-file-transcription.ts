import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

// Mirrors core::file_transcription::FILE_PHASE_*. Phases, progress and
// status strings are decided in core (TASK-91); this hook only polls
// the snapshot and forwards user intents.
export const FILE_PHASE_IDLE = 0;
export const FILE_PHASE_DECODING = 1;
export const FILE_PHASE_LOADING_MODEL = 2;
export const FILE_PHASE_TRANSCRIBING = 3;
export const FILE_PHASE_DONE = 4;
export const FILE_PHASE_ERROR = 5;
export const FILE_PHASE_CANCELLED = 6;

export interface FileJobSnapshot {
  phase: number;
  busy: boolean;
  status: string;
  path: string;
  file_name: string;
  progress: number;
  chunks_done: number;
  chunks_total: number;
  audio_ms: number;
  elapsed_ms: number;
  text: string;
  confidence: number;
  error: string;
  cancelling: boolean;
}

const EMPTY: FileJobSnapshot = {
  phase: FILE_PHASE_IDLE,
  busy: false,
  status: "",
  path: "",
  file_name: "",
  progress: 0,
  chunks_done: 0,
  chunks_total: 0,
  audio_ms: 0,
  elapsed_ms: 0,
  text: "",
  confidence: 0,
  error: "",
  cancelling: false,
};

// 5 Hz is plenty for a progress bar that moves once per ~30 s chunk;
// polling only runs while a job is busy.
const POLL_MS = 200;

export interface FileTranscription {
  snapshot: FileJobSnapshot;
  /** Start a job for an absolute path (drag-drop or picker). */
  start: (path: string) => Promise<void>;
  /** Open the native picker filtered to supported formats, then start. */
  pick: () => Promise<void>;
  cancel: () => Promise<void>;
  /** Error from the start call itself (missing file, already busy). */
  startError: string | null;
}

export function useFileTranscription(): FileTranscription {
  const [snapshot, setSnapshot] = useState<FileJobSnapshot>(EMPTY);
  const [startError, setStartError] = useState<string | null>(null);
  const extensionsRef = useRef<string[] | null>(null);
  const mountedRef = useRef(true);

  const refresh = useCallback(async () => {
    try {
      const snap = await invoke<FileJobSnapshot | null>("file_transcribe_snapshot");
      if (mountedRef.current && snap) setSnapshot(snap);
    } catch {
      // Backend unavailable; keep the last snapshot.
    }
  }, []);

  // Job state lives in core, so a job started before navigating away is
  // picked up again on remount.
  useEffect(() => {
    mountedRef.current = true;
    void refresh();
    return () => {
      mountedRef.current = false;
    };
  }, [refresh]);

  useEffect(() => {
    if (!snapshot.busy) return;
    const id = setInterval(() => void refresh(), POLL_MS);
    return () => clearInterval(id);
  }, [snapshot.busy, refresh]);

  const start = useCallback(
    async (path: string) => {
      setStartError(null);
      try {
        await invoke("file_transcribe_start", { path });
      } catch (e) {
        setStartError(String(e));
      }
      await refresh();
    },
    [refresh],
  );

  const pick = useCallback(async () => {
    if (!extensionsRef.current) {
      try {
        extensionsRef.current =
          (await invoke<string[] | null>("file_transcribe_extensions")) ?? [];
      } catch {
        extensionsRef.current = [];
      }
    }
    const extensions = extensionsRef.current;
    const selected = await open({
      multiple: false,
      directory: false,
      filters:
        extensions.length > 0 ? [{ name: "Audio or video", extensions }] : undefined,
    });
    if (typeof selected === "string") await start(selected);
  }, [start]);

  const cancel = useCallback(async () => {
    await invoke("file_transcribe_cancel").catch(() => {});
    await refresh();
  }, [refresh]);

  return { snapshot, start, pick, cancel, startError };
}

/** "1:05" / "1:02:03" for a millisecond duration. */
export function formatDuration(ms: number): string {
  const total = Math.round(ms / 1000);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const ss = String(s).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${ss}` : `${m}:${ss}`;
}
