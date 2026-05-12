// Public types + format helpers for the crash inspector. The
// stateful piece — polling + mutators + hooks — lives in
// `crashes-store.ts` so a single shared store backs every consumer
// (sidebar dot + Diagnostics overview entry card + list pane). This
// file re-exports the hooks under their original names so existing
// import sites keep working without code edits.

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

export type {
  CrashesSnapshot,
} from "./crashes-store";

export {
  useCrashes,
  useCrashesUnreadCount,
  refetchCrashes,
} from "./crashes-store";

/// Result type the public hooks expose. Kept here for callers that
/// pulled it from `use-crashes.ts` historically.
export interface UseCrashesResult {
  list: CrashSummary[];
  unreadCount: number;
  lastSeenUnreadCount: number;
  loading: boolean;
  error: string | null;
  refetch: () => void;
  markRead: (id: string) => Promise<void>;
  markSeen: () => Promise<void>;
  deleteOne: (id: string) => Promise<void>;
  deleteAll: () => Promise<void>;
  read: (id: string) => Promise<CrashFile>;
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
