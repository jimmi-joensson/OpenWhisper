// Module-level shared store for crash inspector state.
//
// Why this exists: the sidebar's unread-dot poller and the crash list
// pane's polling were running on independent 2 s clocks. After a
// `crashes_mark_read`, the list refetched immediately while the
// sidebar caught up only on its next tick — visible 0–2 s lag where
// the row dot vanished but the rail dot held. Lifting state into a
// module-scope store keyed by `useSyncExternalStore` lets every
// consumer (sidebar, Diagnostics overview, list pane) re-render the
// same React commit when a mutation lands.
//
// Pattern mirrors `useMemoryStats`'s shared store at
// `apps/tauri/src/lib/use-memory-stats.ts` so the codebase has one
// way to do "single poller + many subscribers." Not Zustand — we
// don't import a state-management library, we use the React 18
// primitive that's already pulled in.

import { useCallback, useSyncExternalStore } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { CrashFile, CrashSummary } from "./use-crashes";

const POLL_MS = 2000;

export interface CrashesSnapshot {
  list: CrashSummary[];
  unreadCount: number;
  /// Persisted unread count from the last user "see" event (entering
  /// the crash inspector or acknowledging the launch toast). The
  /// delta-driven launch toast compares this against `unreadCount`:
  /// strict `unreadCount > lastSeenUnreadCount` fires the toast.
  /// Initialised from `crashes_get_last_seen_unread` on first refetch.
  lastSeenUnreadCount: number;
  loading: boolean;
  error: string | null;
}

const INITIAL_SNAPSHOT: CrashesSnapshot = {
  list: [],
  unreadCount: 0,
  lastSeenUnreadCount: 0,
  loading: true,
  error: null,
};

interface StoreState {
  snapshot: CrashesSnapshot;
  listeners: Set<() => void>;
  pollId: ReturnType<typeof setInterval> | null;
  visibilityHandler: (() => void) | null;
}

const store: StoreState = {
  snapshot: INITIAL_SNAPSHOT,
  listeners: new Set(),
  pollId: null,
  visibilityHandler: null,
};

function notify() {
  for (const fn of store.listeners) fn();
}

function pushSnapshot(next: Partial<CrashesSnapshot>) {
  store.snapshot = { ...store.snapshot, ...next };
  notify();
}

async function refetch() {
  try {
    const [list, unreadCount, lastSeenUnreadCount] = await Promise.all([
      invoke<CrashSummary[]>("crashes_list"),
      invoke<number>("crashes_unread_count"),
      invoke<number>("crashes_get_last_seen_unread"),
    ]);
    pushSnapshot({
      list,
      unreadCount,
      lastSeenUnreadCount,
      loading: false,
      error: null,
    });
  } catch (e) {
    pushSnapshot({
      loading: false,
      error: e instanceof Error ? e.message : String(e),
    });
  }
}

function isHidden(): boolean {
  return typeof document !== "undefined" && document.hidden === true;
}

function armPoll() {
  if (store.pollId !== null || isHidden()) return;
  // Don't double-fire on re-arm: the visibility-pause path already
  // calls `refetch` synchronously below. Plain (re)arm just sets
  // the interval.
  store.pollId = setInterval(() => {
    void refetch();
  }, POLL_MS);
}

function disarmPoll() {
  if (store.pollId !== null) {
    clearInterval(store.pollId);
    store.pollId = null;
  }
}

function startPollingIfNeeded() {
  if (store.listeners.size === 0) return;
  if (store.pollId !== null) return;
  // First subscribe in the lifetime of this WebView OR after a
  // hidden→visible cycle. Fetch once immediately so the rail dot is
  // populated before the first interval tick.
  void refetch();
  armPoll();

  if (store.visibilityHandler === null && typeof document !== "undefined") {
    const handler = () => {
      if (isHidden()) {
        disarmPoll();
      } else if (store.listeners.size > 0) {
        void refetch();
        armPoll();
      }
    };
    store.visibilityHandler = handler;
    document.addEventListener("visibilitychange", handler);
  }
}

function stopPollingIfIdle() {
  if (store.listeners.size > 0) return;
  disarmPoll();
  if (store.visibilityHandler !== null && typeof document !== "undefined") {
    document.removeEventListener("visibilitychange", store.visibilityHandler);
    store.visibilityHandler = null;
  }
}

function subscribe(listener: () => void): () => void {
  store.listeners.add(listener);
  startPollingIfNeeded();
  return () => {
    store.listeners.delete(listener);
    stopPollingIfIdle();
  };
}

function getSnapshot(): CrashesSnapshot {
  return store.snapshot;
}

// ---------------------------------------------------------------
// Mutators — every Tauri call that changes server-side state goes
// through here so the in-memory snapshot updates the same React
// commit and `notify()` fans out to all subscribers (sidebar +
// list + overview) atomically. Optimistic updates are used where
// the success path is overwhelmingly common (mark-read, delete);
// errors fall back to a refetch so the snapshot self-heals.
// ---------------------------------------------------------------

export async function markRead(id: string): Promise<void> {
  // Optimistic: drop unread on the matching row + decrement count.
  const prev = store.snapshot;
  const target = prev.list.find((c) => c.id === id);
  if (target?.unread) {
    pushSnapshot({
      list: prev.list.map((c) =>
        c.id === id ? { ...c, unread: false } : c,
      ),
      unreadCount: Math.max(0, prev.unreadCount - 1),
    });
  }
  try {
    await invoke("crashes_mark_read", { id });
  } catch (e) {
    // Rollback by re-fetching ground truth.
    await refetch();
    throw e;
  }
}

export async function deleteOne(id: string): Promise<void> {
  const prev = store.snapshot;
  const target = prev.list.find((c) => c.id === id);
  pushSnapshot({
    list: prev.list.filter((c) => c.id !== id),
    unreadCount: target?.unread
      ? Math.max(0, prev.unreadCount - 1)
      : prev.unreadCount,
  });
  try {
    await invoke("crashes_delete", { id });
  } catch (e) {
    await refetch();
    throw e;
  }
}

export async function deleteAll(): Promise<void> {
  pushSnapshot({ list: [], unreadCount: 0 });
  try {
    await invoke("crashes_delete_all");
  } catch (e) {
    await refetch();
    throw e;
  }
}

export async function readCrashFile(id: string): Promise<CrashFile> {
  return invoke<CrashFile>("crashes_read", { id });
}

/// Force a fresh fetch — useful after an external trigger that
/// landed a crash file (e.g. the DevTools panel's Trigger panic).
/// The 2 s poll picks it up anyway; this just shortens the wait.
export async function refetchCrashes(): Promise<void> {
  await refetch();
}

/// Acknowledge the current unread count — persist it as
/// `last_seen_unread_count` so subsequent restarts at the same or
/// lower unread count don't re-fire the launch toast. Called when:
/// - the user clicks the Diagnostics → Crashes entry card (the
///   explicit "read" action)
/// - the user clicks View or Dismiss on the launch toast
///
/// Optimistic: update the snapshot first so any open toast hides
/// without waiting for the round-trip; fall back to refetch on
/// error so the snapshot self-heals.
export async function markSeen(): Promise<void> {
  const prev = store.snapshot;
  const target = prev.unreadCount;
  if (prev.lastSeenUnreadCount === target) return;
  pushSnapshot({ lastSeenUnreadCount: target });
  try {
    await invoke("crashes_mark_seen", { count: target });
  } catch (e) {
    await refetch();
    throw e;
  }
}

// ---------------------------------------------------------------
// Hooks
// ---------------------------------------------------------------

/// Full crash-inspector state for panes that render rows + the sheet
/// (CrashList, DiagnosticsOverview entry card). Selector identity
/// is the snapshot itself, so re-renders fire on every store update.
export function useCrashes(): {
  list: CrashSummary[];
  unreadCount: number;
  lastSeenUnreadCount: number;
  loading: boolean;
  error: string | null;
  refetch: () => void;
  markRead: typeof markRead;
  markSeen: typeof markSeen;
  deleteOne: typeof deleteOne;
  deleteAll: typeof deleteAll;
  read: typeof readCrashFile;
} {
  const snapshot = useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
  const refetchCb = useCallback(() => {
    void refetchCrashes();
  }, []);
  return {
    list: snapshot.list,
    unreadCount: snapshot.unreadCount,
    lastSeenUnreadCount: snapshot.lastSeenUnreadCount,
    loading: snapshot.loading,
    error: snapshot.error,
    refetch: refetchCb,
    markRead,
    markSeen,
    deleteOne,
    deleteAll,
    read: readCrashFile,
  };
}

/// Selector hook for the sidebar — only the unread count, so the
/// rail dot doesn't re-render when a non-count field of the
/// snapshot changes (e.g. a new crash with no unread delta because
/// the user already marked it read). React's
/// `useSyncExternalStore` doesn't support selectors directly; we
/// just re-derive on each subscribe and rely on React's bail-out
/// when the returned primitive is unchanged.
export function useCrashesUnreadCount(): number {
  return useSyncExternalStore(
    subscribe,
    () => store.snapshot.unreadCount,
    () => store.snapshot.unreadCount,
  );
}
