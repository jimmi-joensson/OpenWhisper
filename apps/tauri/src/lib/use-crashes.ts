import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

// Mirrors `apps/tauri/src-tauri/src/crashes/mod.rs::CrashSummary`.
export interface CrashSummary {
  id: string;
  ts_unix_ms: number;
  app_version: string;
  os: string;
  message_truncated: string;
  unread: boolean;
  uploaded_at: number | null;
}

// Mirrors `openwhisper_core::crashes::CrashFile` (subset we read on the
// React side; serde-extra fields stay forward-compat).
export interface CrashFile {
  schema_version: number;
  id: string;
  ts_unix_ms: number;
  app_version: string;
  os: string;
  rust_panic: {
    thread_name: string;
    message: string;
    location: string;
    backtrace: string;
  };
  recording_state: {
    status_message_at_crash: string;
    duration_ms: number;
    samples_captured: number;
    model_kind: string | null;
    device_id_hash: string | null;
  } | null;
  events: Array<{
    ts_unix_ms: number;
    kind: string;
    data: unknown;
  }>;
}

const POLL_MS = 2000;

export interface UseCrashesResult {
  list: CrashSummary[];
  unreadCount: number;
  loading: boolean;
  error: string | null;
  refetch: () => void;
  markRead: (id: string) => Promise<void>;
  deleteOne: (id: string) => Promise<void>;
  deleteAll: () => Promise<void>;
  read: (id: string) => Promise<CrashFile>;
}

/// Polls `crashes_list` + `crashes_unread_count` at 2 Hz while mounted.
/// `enabled = false` skips polling — the Diagnostics overview keeps
/// the unread badge live even when the user is in the crash list pane,
/// but the list pane wants its own poll cadence and doesn't need the
/// overview's.
export function useCrashes(enabled = true): UseCrashesResult {
  const [list, setList] = useState<CrashSummary[]>([]);
  const [unreadCount, setUnreadCount] = useState<number>(0);
  const [loading, setLoading] = useState<boolean>(true);
  const [error, setError] = useState<string | null>(null);
  const [tick, setTick] = useState(0);
  const cancelledRef = useRef(false);

  const refetch = useCallback(() => setTick((n) => n + 1), []);

  useEffect(() => {
    cancelledRef.current = false;
    if (!enabled) {
      setLoading(false);
      return;
    }

    const fetchOnce = async () => {
      try {
        const [rows, count] = await Promise.all([
          invoke<CrashSummary[]>("crashes_list"),
          invoke<number>("crashes_unread_count"),
        ]);
        if (cancelledRef.current) return;
        setList(rows);
        setUnreadCount(count);
        setError(null);
      } catch (e) {
        if (cancelledRef.current) return;
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        if (!cancelledRef.current) setLoading(false);
      }
    };

    void fetchOnce();
    const id = window.setInterval(() => {
      void fetchOnce();
    }, POLL_MS);

    return () => {
      cancelledRef.current = true;
      window.clearInterval(id);
    };
  }, [enabled, tick]);

  const markRead = useCallback(async (id: string) => {
    await invoke("crashes_mark_read", { id });
    refetch();
  }, [refetch]);

  const deleteOne = useCallback(async (id: string) => {
    await invoke("crashes_delete", { id });
    refetch();
  }, [refetch]);

  const deleteAll = useCallback(async () => {
    await invoke("crashes_delete_all");
    refetch();
  }, [refetch]);

  const read = useCallback(async (id: string) => {
    return invoke<CrashFile>("crashes_read", { id });
  }, []);

  return { list, unreadCount, loading, error, refetch, markRead, deleteOne, deleteAll, read };
}

/// Format a relative timestamp for the entry card's sub-line and row
/// timestamps. "just now" / "5 min ago" / "2 hr ago" / "3 days ago".
export function formatRelative(tsUnixMs: number, nowMs: number = Date.now()): string {
  const diff = Math.max(0, nowMs - tsUnixMs);
  const sec = Math.floor(diff / 1000);
  if (sec < 30) return "just now";
  if (sec < 60) return `${sec} sec ago`;
  const min = Math.floor(sec / 60);
  if (min < 60) return `${min} min ago`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr} hr ago`;
  const days = Math.floor(hr / 24);
  if (days === 1) return "1 day ago";
  return `${days} days ago`;
}

/// Format an absolute UTC timestamp for the row's tooltip + the detail
/// sheet header. `2026-05-04 14:33:21 UTC`.
export function formatAbsoluteUtc(tsUnixMs: number): string {
  const d = new Date(tsUnixMs);
  const pad = (n: number) => n.toString().padStart(2, "0");
  return (
    `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())} ` +
    `${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}:${pad(d.getUTCSeconds())} UTC`
  );
}
