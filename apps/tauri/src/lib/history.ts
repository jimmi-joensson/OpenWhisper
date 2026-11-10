// Types and helpers for transcript history (TASK-95). Mirror
// core/src/settings/history.rs, core/src/history/mod.rs and
// src-tauri/src/history.rs.

export interface HistorySettings {
  // Off by default: no dictated text is stored until the user asks.
  enabled: boolean;
  // Stored text older than this many days is removed. 0 = keep.
  retention_days: number;
}

export interface HistoryState {
  settings: HistorySettings;
  // Path of the file the text is stored in, shown as-is.
  database: string;
}

export interface HistoryEntry {
  id: number;
  // When the recording started, Unix epoch milliseconds.
  at_ms: number;
  duration_ms: number;
  word_count: number;
  // The text that was inserted.
  text: string;
  // The transcript before Refine; null unless Refine changed it.
  original: string | null;
  app_id: string | null;
  app_name: string | null;
}

export interface HistoryPage {
  entries: HistoryEntry[];
  // Entries matching the search, before paging.
  matched: number;
  // Entries stored in total.
  stored: number;
}

// The retention choices offered in Settings. 0 = keep until deleted.
export const RETENTION_OPTIONS: { value: string; label: string }[] = [
  { value: "7", label: "7 days" },
  { value: "30", label: "30 days" },
  { value: "90", label: "90 days" },
  { value: "0", label: "Forever" },
];

export function retentionLabel(days: number): string {
  if (days === 0) return "Forever";
  return days === 1 ? "1 day" : `${days} days`;
}

export interface HistoryDay {
  // Local calendar day, as a stable key ("2026-11-10").
  key: string;
  label: string;
  entries: HistoryEntry[];
}

function dayKey(d: Date): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

function dayLabel(d: Date, now: Date): string {
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const day = new Date(d.getFullYear(), d.getMonth(), d.getDate());
  const diffDays = Math.round((today.getTime() - day.getTime()) / 86_400_000);
  if (diffDays === 0) return "Today";
  if (diffDays === 1) return "Yesterday";
  return d.toLocaleDateString(undefined, {
    weekday: "long",
    day: "numeric",
    month: "long",
    ...(d.getFullYear() === now.getFullYear() ? {} : { year: "numeric" }),
  });
}

// Group entries (already newest first) by the local day they were
// recorded on, keeping their order.
export function groupByDay(entries: HistoryEntry[], now: Date = new Date()): HistoryDay[] {
  const days: HistoryDay[] = [];
  for (const entry of entries) {
    const at = new Date(entry.at_ms);
    const key = dayKey(at);
    const last = days[days.length - 1];
    if (last && last.key === key) {
      last.entries.push(entry);
    } else {
      days.push({ key, label: dayLabel(at, now), entries: [entry] });
    }
  }
  return days;
}

export function formatClock(atMs: number): string {
  return new Date(atMs).toLocaleTimeString(undefined, {
    hour: "2-digit",
    minute: "2-digit",
  });
}
