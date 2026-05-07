import { useSyncExternalStore } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// Mirrors `openwhisper_core::telemetry::ProcessMemory`. Bytes; ms epoch.
export interface ProcessMemory {
  rss_bytes: number;
  peak_rss_bytes: number;
  timestamp_unix_ms: number;
}

// Mirrors `openwhisper_core::telemetry::SystemMemory`. Host-wide
// counters so the Diagnostics pane can answer "is the *machine*
// heavy?" without forcing the user to alt-tab to Activity Monitor.
export interface SystemMemory {
  total_bytes: number;
  used_bytes: number;
  available_bytes: number;
  swap_total_bytes: number;
  swap_used_bytes: number;
}

export type LifecycleState =
  | "Unloaded"
  | "Loading"
  | "Loaded"
  | "Active"
  | "Releasing";

// Mirrors `openwhisper_core::telemetry::ModelMemoryRow`.
export interface ModelMemoryRow {
  label: string;
  state: LifecycleState;
  estimated_rss_bytes: number;
  /// Static weight footprint reported by the model loader. For
  /// in-process models this overlaps with `estimated_rss_bytes`
  /// (claimed_bytes is the more accurate figure, since RSS-delta
  /// can miss concurrent allocations during load). For
  /// out-of-process models (Mac ANE, GPU VRAM) this is the only
  /// place the real footprint surfaces — RSS-delta is near zero.
  claimed_bytes: number;
  /// `true` when `claimed_bytes` is already counted inside the
  /// process RSS. `false` when it lives in an external pool (Mac
  /// ANE) and must be added on top of RSS for a true total.
  in_process: boolean;
}

// Mirrors `openwhisper_core::telemetry::MemoryStats`.
export interface MemoryStats {
  system: SystemMemory;
  process: ProcessMemory;
  models: ModelMemoryRow[];
}

export interface ModelStateChangedPayload {
  label: string;
  state: LifecycleState;
}

const POLL_MS = 1000;
// 60 samples × 1 Hz = 60-second sparkline window. Matches the design's
// "Last 60 s" caption.
export const RSS_SERIES_LEN = 60;

export type PressureLevel = "normal" | "warning" | "critical";

interface UseMemoryStatsResult {
  stats: MemoryStats | null;
  openWhisperSeries: number[];
  systemSeries: number[];
  pressure: PressureLevel;
  error: string | null;
  /// `performance.now()` timestamp of the most recent ring push, or
  /// `null` if no sample has landed yet. The Sparkline component
  /// uses this to anchor its slide-animation clock so re-mounting
  /// the pane (e.g. after navigating away) doesn't visibly snap
  /// the curve — the chart picks up at the same slide position the
  /// previous mount would have shown.
  lastSampleTimeMs: number | null;
}

/// Sum claimed bytes from out-of-process model rows — Mac ANE
/// weights are the canonical case. Used to compute the
/// system-wide-honest "OpenWhisper Memory" total alongside
/// process RSS.
export function externalClaim(models: ModelMemoryRow[]): number {
  let total = 0;
  for (const m of models) {
    if (!m.in_process && m.claimed_bytes > 0) total += m.claimed_bytes;
  }
  return total;
}

/// True when the recognizer model is currently resident *inside* this
/// process — Loaded / Active / mid-transition AND `in_process === true`.
/// On Mac the recognizer runs on the ANE in a separate pool, so this
/// stays false even while the model is fully loaded. The RSS breakdown
/// estimator gates the Parakeet-weights segment on this so the bar
/// doesn't claim ANE-resident weights are inside RSS.
export function isRecognizerInProcessResident(
  models: ModelMemoryRow[],
): boolean {
  for (const m of models) {
    if (m.label !== "recognizer") continue;
    if (!m.in_process) return false;
    return (
      m.state === "Loaded" ||
      m.state === "Active" ||
      m.state === "Loading" ||
      m.state === "Releasing"
    );
  }
  return false;
}

export interface RssBreakdownMb {
  parakeetMb: number;
  audioBuffersMb: number;
  appShellMb: number;
  cachesMb: number;
}

/// V1 placeholder estimator — splits process RSS into the four
/// canonical segments the design expects. Returns megabytes summing
/// (modulo rounding) to `rssMb`.
///
/// Parakeet weights take a static 612 MB segment ONLY when the
/// recognizer is in-process resident (Windows shape). On Mac the
/// weights live in the ANE pool outside RSS, so the segment drops
/// to zero and the per-model breakdown bar below this one carries
/// the ANE attribution. Audio buffers / app shell / caches share
/// the non-Parakeet residual at 28 / 56 / 16 — ratios derived from
/// the design's Windows-shape fixture (142 / 286 / 84 of the 512 MB
/// non-Parakeet residual at rss=1100 MB). Proportional rather than
/// fixed so the segments stay honest at any RSS — including the
/// ~110 MB Mac case where fixed baselines would oversaturate.
export function breakdownEstimate(
  rssMb: number,
  recognizerInProcessResident: boolean,
): RssBreakdownMb {
  const parakeetMb = recognizerInProcessResident ? 612 : 0;
  const remainder = Math.max(0, rssMb - parakeetMb);
  const audioBuffersMb = Math.round(remainder * 0.28);
  const cachesMb = Math.round(remainder * 0.16);
  const appShellMb = Math.max(0, remainder - audioBuffersMb - cachesMb);
  return { parakeetMb, audioBuffersMb, appShellMb, cachesMb };
}

/// Total memory OpenWhisper holds across all OS-managed pools.
/// Equals process RSS (which already counts in-process model
/// weights) plus the claim from ANE-resident weights on Mac.
export function openWhisperTotalBytes(stats: MemoryStats): number {
  return stats.process.rss_bytes + externalClaim(stats.models);
}

// Cross-platform pressure proxy. The OS-native pressure signals
// (macOS `kern.memorystatus_vm_pressure_level`, Windows
// `GetPerformanceInfo().MemoryLoad`) are RATE-based — they look at
// page-out activity, compressor pressure, and reclaim demand, not at
// the headline used/total ratio. `sysinfo` 0.32 surfaces neither, so
// we approximate from the only cross-platform numbers we have.
//
// Important macOS quirk: macOS aggressively caches in RAM and
// pre-commits swap, so a healthy idle Mac routinely sits at 80–90%
// used and 90%+ swap committed without the system being "under
// pressure" in any operational sense. Activity Monitor still shows
// "green" on those machines because pressure is rate-based, not
// usage-based.
//
// Therefore:
//   - We do NOT use swap_used/swap_total. Committed swap on macOS is
//     a poor pressure signal; including it produces false-criticals.
//   - We use used/total with thresholds calibrated against Activity
//     Monitor / Task Manager guidance (commonly cited: <85% normal,
//     85–95% warning territory, ≥95% critical):
//
//       - `critical` if used/total ≥ 0.95
//       - `warning`  if used/total ≥ 0.88
//       - `normal`   otherwise
//
// This matches the design's mock (21 GB / 24 GB = 0.875 → Normal).
// A user genuinely thrashing memory (≥95%) still flips to red. Real
// memory pressure (page-out rate, compressor activity) is a future
// upgrade once we plumb platform-native APIs through `core::telemetry`.
function derivePressure(s: SystemMemory): PressureLevel {
  if (s.total_bytes === 0) return "normal";
  const usedRatio = s.used_bytes / s.total_bytes;
  if (usedRatio >= 0.95) return "critical";
  if (usedRatio >= 0.88) return "warning";
  return "normal";
}

// ---------------------------------------------------------------
// Module-level store — so the ring buffer survives pane navigation
// and keeps filling while the user is on Home / Settings.
// ---------------------------------------------------------------
//
// The poll auto-starts on first import (see `ensurePollStarted` at
// bottom). The Diagnostics pane subscribes via `useSyncExternalStore`
// and gets snapshots that include the historical ring — so leaving
// Diagnostics, loading a model, and coming back shows the load
// event as a step in the chart, not a freshly empty graph.
//
// Cost: one IPC call per second for the lifetime of the WebView.
// Negligible. Memory: 60 × 2 numbers ≈ 1 KB plus the latest stats
// object.

interface StoreState {
  snapshot: UseMemoryStatsResult;
  // Mutable ring buffers — `snapshot.openWhisperSeries` /
  // `snapshot.systemSeries` are fresh array copies handed to
  // subscribers, but writes go through these refs.
  openWhisperRing: number[];
  systemRing: number[];
  listeners: Set<() => void>;
  pollId: ReturnType<typeof setInterval> | null;
  unlistenStateChange: UnlistenFn | undefined;
  cancelled: boolean;
  // Visibility-driven pause control. The interval is torn down
  // entirely when the window is hidden (minimized, app in tray,
  // screen locked) and re-armed on resume. Saves the per-second
  // IPC for however long the user can't see the chart anyway.
  visibilityHandler: (() => void) | null;
}

const INITIAL_SNAPSHOT: UseMemoryStatsResult = {
  stats: null,
  openWhisperSeries: [],
  systemSeries: [],
  pressure: "normal",
  error: null,
  lastSampleTimeMs: null,
};

const store: StoreState = {
  snapshot: INITIAL_SNAPSHOT,
  openWhisperRing: [],
  systemRing: [],
  listeners: new Set(),
  pollId: null,
  unlistenStateChange: undefined,
  cancelled: false,
  visibilityHandler: null,
};

function notify() {
  for (const fn of store.listeners) fn();
}

function pushSnapshot(next: Partial<UseMemoryStatsResult>) {
  store.snapshot = { ...store.snapshot, ...next };
  notify();
}

// Two refresh paths, deliberately separate.
//
// - Poll: fires once per `POLL_MS`. Pushes both series rings AND
//   updates the readout/breakdown stats. This is the only writer
//   of the time-series — guarantees a fixed 1 Hz sample cadence
//   even when state events flurry.
// - Event: fires on every `model-state-changed`. Refreshes the
//   readout/breakdown stats (so the State column and breakdown
//   segments update instantly between polls) but does NOT push
//   to the rings. Without this split, a Loaded→Active→Loaded→
//   Releasing→Loaded burst on stop-recording would compress
//   several "samples" into a sub-second window and the
//   sparkline visibly speeds up while the burst clears.
async function fetchStats(pushToRing: boolean) {
  try {
    const next = await invoke<MemoryStats>("telemetry_get_memory");
    if (store.cancelled) return;

    if (pushToRing) {
      // Push the system-wide-honest total (process RSS + ANE
      // claim) instead of raw RSS — the sparkline tracks
      // "OpenWhisper Memory" as the user experiences it.
      store.openWhisperRing.push(openWhisperTotalBytes(next));
      if (store.openWhisperRing.length > RSS_SERIES_LEN) {
        store.openWhisperRing.splice(
          0,
          store.openWhisperRing.length - RSS_SERIES_LEN,
        );
      }
      store.systemRing.push(next.system.used_bytes);
      if (store.systemRing.length > RSS_SERIES_LEN) {
        store.systemRing.splice(
          0,
          store.systemRing.length - RSS_SERIES_LEN,
        );
      }
      pushSnapshot({
        stats: next,
        openWhisperSeries: [...store.openWhisperRing],
        systemSeries: [...store.systemRing],
        pressure: derivePressure(next.system),
        error: null,
        lastSampleTimeMs: performance.now(),
      });
    } else {
      pushSnapshot({
        stats: next,
        pressure: derivePressure(next.system),
        error: null,
      });
    }
  } catch (e) {
    if (store.cancelled) return;
    pushSnapshot({ error: String(e) });
  }
}

let pollStarted = false;

function isHidden(): boolean {
  return typeof document !== "undefined" && document.hidden === true;
}

// Arm the 1 Hz interval. No-op if already armed or if the document
// is hidden — in the hidden case we just wait for the next
// `visibilitychange` to fire `armPoll` again. Always fires one
// immediate fetch on (re-)arm so the readout reflects current
// state without waiting for the first tick.
function armPoll() {
  if (store.cancelled || store.pollId !== null || isHidden()) return;
  void fetchStats(true);
  store.pollId = setInterval(() => {
    void fetchStats(true);
  }, POLL_MS);
}

function disarmPoll() {
  if (store.pollId !== null) {
    clearInterval(store.pollId);
    store.pollId = null;
  }
}

/// Start the 1 Hz memory poll. Idempotent — safe to call from
/// multiple call sites (e.g. App boot AND first hook subscribe).
/// The first call wins; subsequent calls are no-ops.
///
/// The poll auto-pauses when `document.hidden` flips to `true`
/// (window minimized / app in tray / screen locked) and resumes on
/// `visibilitychange` → visible. The ring buffer is preserved
/// across pauses; only the rate of new samples drops to zero.
export function startMemoryStatsPoll(): void {
  if (pollStarted) return;
  pollStarted = true;
  store.cancelled = false;

  // Arm now if the window is visible; otherwise the
  // visibilitychange listener below will arm on first show.
  armPoll();

  if (typeof document !== "undefined") {
    const handler = () => {
      if (store.cancelled) return;
      if (isHidden()) {
        disarmPoll();
      } else {
        armPoll();
      }
    };
    document.addEventListener("visibilitychange", handler);
    store.visibilityHandler = handler;
  }

  // Event-driven refresh stays attached even while hidden — it's
  // free (no IPC unless something fires) and cheap to handle. If
  // a state change does fire while hidden, we refresh stats so the
  // breakdown reflects reality the moment the user shows the
  // window again.
  void listen<ModelStateChangedPayload>("model-state-changed", () => {
    void fetchStats(false);
  }).then((unlisten) => {
    if (store.cancelled) {
      unlisten();
    } else {
      store.unlistenStateChange = unlisten;
    }
  });
}

/// Tear down the poll + event listener. Used by tests; production
/// callers don't need to stop the poll — the WebView's lifetime is
/// the natural boundary.
export function stopMemoryStatsPoll(): void {
  store.cancelled = true;
  pollStarted = false;
  disarmPoll();
  store.unlistenStateChange?.();
  store.unlistenStateChange = undefined;
  if (typeof document !== "undefined" && store.visibilityHandler) {
    document.removeEventListener("visibilitychange", store.visibilityHandler);
  }
  store.visibilityHandler = null;
}

function subscribe(fn: () => void): () => void {
  store.listeners.add(fn);
  return () => {
    store.listeners.delete(fn);
  };
}

function getSnapshot(): UseMemoryStatsResult {
  return store.snapshot;
}

// `useSyncExternalStore` is the React-blessed way to subscribe to
// an external store while staying concurrent-safe. We hand back the
// same `snapshot` reference until something changes, so React's
// `Object.is` bail-out works and there's no spurious re-render.
export function useMemoryStats(): UseMemoryStatsResult {
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}

// Auto-start the poll on module import. The Tauri shell mounts the
// React app once and keeps the WebView alive across navigation, so
// importing this module from anywhere in the app starts a single
// background poll for the lifetime of the window. Result: the ring
// buffer fills up while the user is on Home / Settings, and a
// model load/unload that happens while the user is elsewhere is
// visible the moment they open Diagnostics.
//
// Guarded by `typeof window` so SSR / Node test runners don't try
// to call `setInterval` before the harness has mocked Tauri.
function ensurePollStarted() {
  if (typeof window === "undefined") return;
  startMemoryStatsPoll();
}
ensurePollStarted();
