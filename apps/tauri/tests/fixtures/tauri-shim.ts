import { test as base, expect, type Page } from "@playwright/test";

// Mirrors the DictationTick payload that core/src-tauri emits at 20 Hz.
export interface MockTick {
  phase: number;
  status: "idle" | "recording" | "transcribing";
  status_message?: string;
  transcript?: string;
  confidence?: number;
  sample_count?: number;
  elapsed_ms?: number;
  error_message?: string;
  can_toggle?: boolean;
  is_recording?: boolean;
  level?: number;
  download_bytes_done?: number;
  download_bytes_total?: number;
}

export const TICK_DEFAULTS: Required<Omit<MockTick, "phase" | "status">> = {
  status_message: "",
  transcript: "",
  confidence: 0,
  sample_count: 0,
  elapsed_ms: 0,
  error_message: "",
  can_toggle: true,
  is_recording: false,
  level: 0,
  download_bytes_done: 0,
  download_bytes_total: 0,
};

declare global {
  interface Window {
    __owEmit: (event: string, payload: unknown) => number;
    __TAURI_INTERNALS__: unknown;
    __TAURI_EVENT_PLUGIN_INTERNALS__: unknown;
  }
}

// Install the Tauri 2 internals stub before the SPA boots. Records emitted
// `dictation_tick` events into a queue the test can replay.
async function installTauriShim(page: Page, label: "main" | "pill" = "main") {
  await page.addInitScript((windowLabel) => {
    const handlers = new Map<string, Set<number>>();
    const callbacks = new Map<number, (payload: unknown) => void>();
    let nextId = 1;
    let tickCounter = 0;

    window.__TAURI_INTERNALS__ = {
      metadata: {
        currentWindow: { label: windowLabel },
        currentWebview: { label: windowLabel, windowLabel },
      },
      invoke: async (cmd: string, args?: Record<string, unknown>) => {
        // plugin:window|<verb> — record + return sane defaults so the
        // WindowControls component can call minimize/toggleMaximize/close
        // without a real Tauri runtime. Verb is the suffix after the pipe.
        if (cmd.startsWith("plugin:window|")) {
          const verb = cmd.slice("plugin:window|".length);
          const w = window as unknown as {
            __owWindowCalls?: string[];
            __owMaximized?: boolean;
          };
          w.__owWindowCalls = w.__owWindowCalls ?? [];
          w.__owWindowCalls.push(verb);
          if (verb === "is_maximized") return w.__owMaximized ?? false;
          if (verb === "toggle_maximize") {
            w.__owMaximized = !(w.__owMaximized ?? false);
            return null;
          }
          return null;
        }
        if (cmd === "core_version") return "0.1.0-test";
        if (cmd === "plugin:app|name") return "OpenWhisper Dev";
        if (cmd === "dictation_toggle") return null;
        if (cmd === "dictation_cancel") return false;
        if (cmd === "hotkey_status_current") return null;
        if (cmd === "permissions_status_current") return null;
        if (cmd === "hotkey_retry") {
          (window as unknown as { __owHotkeyRetryCount?: number }).__owHotkeyRetryCount =
            ((window as unknown as { __owHotkeyRetryCount?: number }).__owHotkeyRetryCount ?? 0) + 1;
          return null;
        }
        if (cmd === "open_settings_window") {
          (window as unknown as { __owOpenSettingsCount?: number }).__owOpenSettingsCount =
            ((window as unknown as { __owOpenSettingsCount?: number }).__owOpenSettingsCount ?? 0) + 1;
          return null;
        }
        if (cmd === "settings_get_hotkeys") {
          const stored = (window as unknown as { __owHotkeys?: unknown }).__owHotkeys;
          return (
            stored ?? {
              toggle: { kind: "modifier-tap", code: "RightCommand", mods: [] },
              cancel: { kind: "chord", code: "Escape", mods: [] },
            }
          );
        }
        if (cmd === "settings_set_hotkey") {
          const { target, config } = (args ?? {}) as {
            target: "toggle" | "cancel";
            config: unknown;
          };
          const w = window as unknown as { __owHotkeys?: Record<string, unknown> };
          w.__owHotkeys = w.__owHotkeys ?? {
            toggle: { kind: "modifier-tap", code: "RightCommand", mods: [] },
            cancel: { kind: "chord", code: "Escape", mods: [] },
          };
          w.__owHotkeys[target] = config;
          (window as unknown as { __owHotkeySetCount?: number }).__owHotkeySetCount =
            ((window as unknown as { __owHotkeySetCount?: number }).__owHotkeySetCount ?? 0) + 1;
          (window as unknown as { __owHotkeyLastTarget?: string }).__owHotkeyLastTarget = target;
          return null;
        }
        if (cmd === "settings_reset_hotkey") {
          const { target } = (args ?? {}) as { target: "toggle" | "cancel" };
          const def =
            target === "toggle"
              ? { kind: "modifier-tap", code: "RightCommand", mods: [] }
              : { kind: "chord", code: "Escape", mods: [] };
          const w = window as unknown as { __owHotkeys?: Record<string, unknown> };
          w.__owHotkeys = w.__owHotkeys ?? {
            toggle: { kind: "modifier-tap", code: "RightCommand", mods: [] },
            cancel: { kind: "chord", code: "Escape", mods: [] },
          };
          w.__owHotkeys[target] = def;
          (window as unknown as { __owHotkeyResetCount?: number }).__owHotkeyResetCount =
            ((window as unknown as { __owHotkeyResetCount?: number }).__owHotkeyResetCount ?? 0) + 1;
          (window as unknown as { __owHotkeyLastTarget?: string }).__owHotkeyLastTarget = target;
          return def;
        }
        if (cmd === "settings_capture_hotkey_start") {
          const { target } = (args ?? {}) as { target?: "toggle" | "cancel" };
          (window as unknown as { __owCaptureStartCount?: number }).__owCaptureStartCount =
            ((window as unknown as { __owCaptureStartCount?: number }).__owCaptureStartCount ?? 0) + 1;
          (window as unknown as { __owCaptureLastTarget?: string }).__owCaptureLastTarget = target;
          return null;
        }
        if (cmd === "settings_capture_hotkey_cancel") {
          (window as unknown as { __owCaptureCancelCount?: number }).__owCaptureCancelCount =
            ((window as unknown as { __owCaptureCancelCount?: number }).__owCaptureCancelCount ?? 0) + 1;
          return null;
        }
        if (cmd === "audio_get_device_state") {
          // Mirror the Rust shell's snapshot shape. Tests stash device
          // fixtures on `__owAudioDevices` and the saved selection on
          // `__owAudioDevice` (the cpal id). Defaults match the seed
          // pair below so a test that just opens the pane gets a sane
          // pre-populated picker.
          const w = window as unknown as {
            __owAudioDevices?: Array<{
              id: string;
              label: string;
              is_default: boolean;
            }>;
            __owAudioDevice?: string | null;
            __owAudioDefaultLabel?: string | null;
            __owAudioSelectedPresent?: boolean;
          };
          const devices = w.__owAudioDevices ?? [
            {
              id: "default-mic",
              label: "MacBook Pro Microphone",
              is_default: true,
            },
            { id: "airpods-pro", label: "AirPods Pro", is_default: false },
          ];
          const selectedId = w.__owAudioDevice ?? null;
          const defaultLabel =
            w.__owAudioDefaultLabel ??
            devices.find((d) => d.is_default)?.label ??
            null;
          const selectedPresent =
            w.__owAudioSelectedPresent ??
            (selectedId === null || devices.some((d) => d.id === selectedId));
          return {
            devices,
            selected_id: selectedId,
            selected_present: selectedPresent,
            default_label: defaultLabel,
          };
        }
        if (cmd === "audio_set_device") {
          const { id } = (args ?? {}) as { id: string | null };
          const w = window as unknown as {
            __owAudioDevice?: string | null;
            __owAudioSetCount?: number;
            __owAudioLastSet?: string | null;
          };
          w.__owAudioDevice = id;
          w.__owAudioLastSet = id;
          w.__owAudioSetCount = (w.__owAudioSetCount ?? 0) + 1;
          return null;
        }
        if (cmd === "behavior_get_show_in_fullscreen") {
          const w = window as unknown as { __owShowInFullscreen?: boolean };
          return w.__owShowInFullscreen ?? false;
        }
        if (cmd === "behavior_set_show_in_fullscreen") {
          const { enabled } = (args ?? {}) as { enabled: boolean };
          const w = window as unknown as {
            __owShowInFullscreen?: boolean;
            __owShowInFullscreenLastSet?: boolean;
            __owShowInFullscreenSetCount?: number;
          };
          w.__owShowInFullscreen = enabled;
          w.__owShowInFullscreenLastSet = enabled;
          w.__owShowInFullscreenSetCount =
            (w.__owShowInFullscreenSetCount ?? 0) + 1;
          return null;
        }
        if (cmd === "behavior_get_pause_audio_during_dictation") {
          const w = window as unknown as { __owPauseAudio?: boolean };
          return w.__owPauseAudio ?? true;
        }
        if (cmd === "behavior_set_pause_audio_during_dictation") {
          const { enabled } = (args ?? {}) as { enabled: boolean };
          const w = window as unknown as {
            __owPauseAudio?: boolean;
            __owPauseAudioLastSet?: boolean;
            __owPauseAudioSetCount?: number;
          };
          w.__owPauseAudio = enabled;
          w.__owPauseAudioLastSet = enabled;
          w.__owPauseAudioSetCount = (w.__owPauseAudioSetCount ?? 0) + 1;
          return null;
        }
        if (cmd === "behavior_get_bt_resume_delay_ms") {
          const w = window as unknown as { __owBtResumeDelayMs?: number };
          return w.__owBtResumeDelayMs ?? 5000;
        }
        if (cmd === "behavior_set_bt_resume_delay_ms") {
          const { delayMs } = (args ?? {}) as { delayMs: number };
          const clamped = Math.min(Math.max(delayMs, 0), 10000);
          const w = window as unknown as {
            __owBtResumeDelayMs?: number;
            __owBtResumeDelayLastSet?: number;
            __owBtResumeDelaySetCount?: number;
          };
          w.__owBtResumeDelayMs = clamped;
          w.__owBtResumeDelayLastSet = clamped;
          w.__owBtResumeDelaySetCount =
            (w.__owBtResumeDelaySetCount ?? 0) + 1;
          return null;
        }
        if (cmd === "audio_preview_start") {
          const w = window as unknown as { __owAudioPreviewStarts?: number };
          w.__owAudioPreviewStarts = (w.__owAudioPreviewStarts ?? 0) + 1;
          return null;
        }
        if (cmd === "audio_preview_stop") {
          const w = window as unknown as { __owAudioPreviewStops?: number };
          w.__owAudioPreviewStops = (w.__owAudioPreviewStops ?? 0) + 1;
          return null;
        }
        if (cmd === "plugin:autostart|is_enabled") {
          const w = window as unknown as {
            __owAutostart?: boolean;
            __owAutostartIsEnabledShouldThrow?: boolean;
          };
          if (w.__owAutostartIsEnabledShouldThrow) {
            throw new Error("autostart isEnabled failed");
          }
          return w.__owAutostart ?? false;
        }
        if (cmd === "plugin:autostart|enable") {
          const w = window as unknown as {
            __owAutostart?: boolean;
            __owAutostartEnableShouldThrow?: boolean;
            __owAutostartEnableCount?: number;
            __owAutostartLastSet?: boolean;
          };
          w.__owAutostartEnableCount = (w.__owAutostartEnableCount ?? 0) + 1;
          if (w.__owAutostartEnableShouldThrow) {
            throw new Error("autostart enable failed");
          }
          w.__owAutostart = true;
          w.__owAutostartLastSet = true;
          return null;
        }
        if (cmd === "plugin:autostart|disable") {
          const w = window as unknown as {
            __owAutostart?: boolean;
            __owAutostartDisableShouldThrow?: boolean;
            __owAutostartDisableCount?: number;
            __owAutostartLastSet?: boolean;
          };
          w.__owAutostartDisableCount = (w.__owAutostartDisableCount ?? 0) + 1;
          if (w.__owAutostartDisableShouldThrow) {
            throw new Error("autostart disable failed");
          }
          w.__owAutostart = false;
          w.__owAutostartLastSet = false;
          return null;
        }
        if (cmd === "settings_get_pill") {
          const stored = (window as unknown as { __owPillFollow?: boolean })
            .__owPillFollow;
          return { follow_active_screen: stored ?? true };
        }
        if (cmd === "stats_get_summary") {
          const w = window as unknown as {
            __owStatsSummary?: {
              words_today: number;
              words_week: number;
              words_all_time: number;
              seconds_total: number;
            };
            __owStatsGetCount?: number;
          };
          w.__owStatsGetCount = (w.__owStatsGetCount ?? 0) + 1;
          return (
            w.__owStatsSummary ?? {
              words_today: 0,
              words_week: 0,
              words_all_time: 0,
              seconds_total: 0,
            }
          );
        }
        if (cmd === "stats_reset") {
          const w = window as unknown as { __owStatsResetCount?: number };
          w.__owStatsResetCount = (w.__owStatsResetCount ?? 0) + 1;
          return null;
        }
        if (cmd === "settings_get_stats") {
          const w = window as unknown as { __owUserWpm?: number };
          return { user_wpm: w.__owUserWpm ?? 40 };
        }
        if (cmd === "settings_set_user_wpm") {
          const { wpm } = (args ?? {}) as { wpm: number };
          const clamped = Math.max(10, Math.min(300, wpm));
          const w = window as unknown as {
            __owUserWpm?: number;
            __owUserWpmSetCount?: number;
            __owUserWpmLast?: number;
          };
          w.__owUserWpm = clamped;
          w.__owUserWpmSetCount = (w.__owUserWpmSetCount ?? 0) + 1;
          w.__owUserWpmLast = clamped;
          return clamped;
        }
        if (cmd === "telemetry_get_memory") {
          const w = window as unknown as {
            __owMemoryStats?: MockMemoryStats;
            __owTelemetryGetCount?: number;
          };
          w.__owTelemetryGetCount = (w.__owTelemetryGetCount ?? 0) + 1;
          const stash = w.__owMemoryStats;
          if (!stash) {
            return {
              system: {
                total_bytes: 0,
                used_bytes: 0,
                available_bytes: 0,
                swap_total_bytes: 0,
                swap_used_bytes: 0,
              },
              process: {
                rss_bytes: 0,
                peak_rss_bytes: 0,
                timestamp_unix_ms: 0,
              },
              models: [],
            };
          }
          // Fill in claimed_bytes / in_process defaults so legacy
          // fixtures that predate ModelClaim keep working: missing
          // in_process → true, missing claimed_bytes → 0. Specs that
          // test ANE-resident memory must set both explicitly.
          return {
            ...stash,
            models: stash.models.map((m) => ({
              label: m.label,
              state: m.state,
              estimated_rss_bytes: m.estimated_rss_bytes,
              claimed_bytes: m.claimed_bytes ?? 0,
              in_process: m.in_process ?? true,
            })),
          };
        }
        if (cmd === "settings_get_performance") {
          const w = window as unknown as { __owKeepModelsWarm?: boolean };
          return { keep_models_warm: w.__owKeepModelsWarm ?? false };
        }
        if (cmd === "settings_set_keep_models_warm") {
          const { value } = (args ?? {}) as { value: boolean };
          const w = window as unknown as {
            __owKeepModelsWarm?: boolean;
            __owKeepModelsWarmSetCount?: number;
            __owKeepModelsWarmLast?: boolean;
            __owKeepModelsWarmShouldThrow?: boolean;
          };
          if (w.__owKeepModelsWarmShouldThrow) {
            throw new Error("settings_set_keep_models_warm failed");
          }
          w.__owKeepModelsWarm = value;
          w.__owKeepModelsWarmLast = value;
          w.__owKeepModelsWarmSetCount =
            (w.__owKeepModelsWarmSetCount ?? 0) + 1;
          return null;
        }
        if (cmd === "settings_set_pill_follow") {
          const { follow } = (args ?? {}) as { follow: boolean };
          const w = window as unknown as {
            __owPillFollow?: boolean;
            __owPillSetCount?: number;
            __owPillLastFollow?: boolean;
          };
          w.__owPillFollow = follow;
          w.__owPillLastFollow = follow;
          w.__owPillSetCount = (w.__owPillSetCount ?? 0) + 1;
          return null;
        }
        if (cmd === "reposition_pill") {
          // PillOverlay's mount-effect calls this; stub returns OK so the
          // catch path doesn't surface in unrelated specs.
          return null;
        }
        if (cmd === "set_pill_click_through") {
          const { passthrough } = (args ?? {}) as { passthrough: boolean };
          const w = window as unknown as {
            __owPillPassthrough?: boolean;
            __owPillPassthroughCalls?: number;
          };
          w.__owPillPassthrough = passthrough;
          w.__owPillPassthroughCalls = (w.__owPillPassthroughCalls ?? 0) + 1;
          return null;
        }
        if (cmd === "show_main_window") {
          const w = window as unknown as { __owShowMainCount?: number };
          w.__owShowMainCount = (w.__owShowMainCount ?? 0) + 1;
          return null;
        }
        if (cmd === "crashes_list") {
          const w = window as unknown as { __owCrashes?: MockCrashSummary[] };
          return (w.__owCrashes ?? []).slice().sort((a, b) => b.ts_unix_ms - a.ts_unix_ms);
        }
        if (cmd === "crashes_unread_count") {
          const w = window as unknown as { __owCrashes?: MockCrashSummary[] };
          return (w.__owCrashes ?? []).filter((c) => c.unread).length;
        }
        if (cmd === "crashes_read") {
          const { id } = (args ?? {}) as { id: string };
          const w = window as unknown as {
            __owCrashes?: MockCrashSummary[];
            __owCrashFiles?: Record<string, MockCrashFile>;
          };
          const file = w.__owCrashFiles?.[id];
          if (file) return file;
          // Synthesize a minimal CrashFile from the summary if no
          // explicit fixture was set — keeps test setup terse.
          const summary = w.__owCrashes?.find((c) => c.id === id);
          if (!summary) {
            throw new Error(`crash ${id} not found`);
          }
          return {
            schema_version: 1,
            id: summary.id,
            ts_unix_ms: summary.ts_unix_ms,
            app_version: summary.app_version,
            os: summary.os,
            rust_panic: {
              thread_name: "main",
              message: summary.message_truncated,
              location: "test:1:1",
              backtrace: "<test fixture>",
            },
            recording_state: null,
            events: [],
          };
        }
        if (cmd === "crashes_mark_read") {
          const { id } = (args ?? {}) as { id: string };
          const w = window as unknown as {
            __owCrashes?: MockCrashSummary[];
            __owCrashesMarkReadCount?: number;
            __owCrashesMarkReadLast?: string;
          };
          if (w.__owCrashes) {
            for (const c of w.__owCrashes) {
              if (c.id === id) c.unread = false;
            }
          }
          w.__owCrashesMarkReadCount = (w.__owCrashesMarkReadCount ?? 0) + 1;
          w.__owCrashesMarkReadLast = id;
          return null;
        }
        if (cmd === "crashes_delete") {
          const { id } = (args ?? {}) as { id: string };
          const w = window as unknown as {
            __owCrashes?: MockCrashSummary[];
            __owCrashesDeleteCount?: number;
            __owCrashesDeleteLast?: string;
          };
          if (w.__owCrashes) {
            w.__owCrashes = w.__owCrashes.filter((c) => c.id !== id);
          }
          w.__owCrashesDeleteCount = (w.__owCrashesDeleteCount ?? 0) + 1;
          w.__owCrashesDeleteLast = id;
          return null;
        }
        if (cmd === "crashes_delete_all") {
          const w = window as unknown as {
            __owCrashes?: MockCrashSummary[];
            __owCrashesDeleteAllCount?: number;
          };
          w.__owCrashes = [];
          w.__owCrashesDeleteAllCount = (w.__owCrashesDeleteAllCount ?? 0) + 1;
          return null;
        }
        if (cmd === "plugin:opener|open_url") {
          const { url } = (args ?? {}) as { url: string };
          const w = window as unknown as {
            __owOpenerOpenUrlCalls?: string[];
          };
          w.__owOpenerOpenUrlCalls = w.__owOpenerOpenUrlCalls ?? [];
          w.__owOpenerOpenUrlCalls.push(url);
          return null;
        }
        if (cmd === "crashes_open_folder") {
          const w = window as unknown as { __owCrashesOpenFolderCount?: number };
          w.__owCrashesOpenFolderCount =
            (w.__owCrashesOpenFolderCount ?? 0) + 1;
          return null;
        }
        if (cmd === "models_storage_path") {
          const w = window as unknown as { __owModelsStoragePath?: string };
          return (
            w.__owModelsStoragePath ??
            "/Users/test/Library/Application Support/com.openwhisper.dev/models"
          );
        }
        if (cmd === "models_open_folder") {
          const w = window as unknown as { __owModelsOpenFolderCount?: number };
          w.__owModelsOpenFolderCount =
            (w.__owModelsOpenFolderCount ?? 0) + 1;
          return null;
        }
        if (cmd === "crashes_debug_trigger_panic") {
          const w = window as unknown as { __owCrashesDebugTriggerCount?: number };
          w.__owCrashesDebugTriggerCount =
            (w.__owCrashesDebugTriggerCount ?? 0) + 1;
          return null;
        }
        if (cmd === "plugin:event|listen") {
          const { event, handler } = (args ?? {}) as {
            event: string;
            handler: number;
          };
          if (!handlers.has(event)) handlers.set(event, new Set());
          handlers.get(event)!.add(handler);
          return handler;
        }
        if (cmd === "plugin:event|unlisten") {
          const { event, eventId } = (args ?? {}) as {
            event: string;
            eventId: number;
          };
          handlers.get(event)?.delete(eventId);
          return null;
        }
        return null;
      },
      transformCallback: (fn: (payload: unknown) => void) => {
        const id = nextId++;
        callbacks.set(id, fn);
        return id;
      },
      unregisterCallback: (id: number) => {
        callbacks.delete(id);
      },
      callbacks,
    } as never;

    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
      unregisterListener: (event: string, eventId: number) => {
        handlers.get(event)?.delete(eventId);
        callbacks.delete(eventId);
      },
    } as never;

    window.__owEmit = (event: string, payload: unknown) => {
      const set = handlers.get(event);
      if (!set) return 0;
      let delivered = 0;
      for (const id of set) {
        const cb = callbacks.get(id);
        if (cb) {
          cb({ event, id: ++tickCounter, payload });
          delivered++;
        }
      }
      return delivered;
    };
  }, label);
}

// Push a `behavior_show_in_fullscreen_changed` event at the
// useShowInFullscreen subscriber. Mirrors the Rust setter's emit.
export async function emitShowInFullscreenChanged(
  page: Page,
  enabled: boolean,
): Promise<number> {
  return page.evaluate(
    (value) => window.__owEmit("behavior_show_in_fullscreen_changed", value),
    enabled,
  );
}

// Push a `behavior_pause_audio_changed` event at the usePauseAudio
// subscriber. Mirrors the Rust setter's emit.
export async function emitPauseAudioChanged(
  page: Page,
  enabled: boolean,
): Promise<number> {
  return page.evaluate(
    (value) => window.__owEmit("behavior_pause_audio_changed", value),
    enabled,
  );
}

// Push a `behavior_bt_resume_delay_changed` event at the
// useBtResumeDelay subscriber. Mirrors the Rust setter's emit.
export async function emitBtResumeDelayChanged(
  page: Page,
  delayMs: number,
): Promise<number> {
  return page.evaluate(
    (value) => window.__owEmit("behavior_bt_resume_delay_changed", value),
    delayMs,
  );
}

export interface MockStatsSummary {
  words_today: number;
  words_week: number;
  words_all_time: number;
  seconds_total: number;
}

/// Stash a stats summary on the window so the next `stats_get_summary`
/// invoke (initial mount or post-event refetch) returns this fixture
/// instead of the all-zeros default.
export async function setStatsSummary(
  page: Page,
  summary: MockStatsSummary,
): Promise<void> {
  await page.evaluate((s) => {
    (window as unknown as { __owStatsSummary?: MockStatsSummary }).__owStatsSummary = s;
  }, summary);
}

/// Fire `stats_changed` so the useStatsSummary hook re-fetches.
export async function emitStatsChanged(page: Page): Promise<number> {
  return page.evaluate(() => window.__owEmit("stats_changed", null));
}

/// Stash the WPM fixture so `settings_get_stats` returns it.
export async function setUserWpm(page: Page, wpm: number): Promise<void> {
  await page.evaluate((value) => {
    (window as unknown as { __owUserWpm?: number }).__owUserWpm = value;
  }, wpm);
}

/// Fire `settings_stats_changed` so the useUserWpm hook updates.
export async function emitSettingsStatsChanged(
  page: Page,
  wpm: number,
): Promise<number> {
  return page.evaluate(
    (value) => window.__owEmit("settings_stats_changed", value),
    wpm,
  );
}

export async function emitTick(page: Page, tick: MockTick): Promise<number> {
  const merged = { ...TICK_DEFAULTS, ...tick };
  return page.evaluate(
    (payload) => window.__owEmit("dictation_tick", payload),
    merged,
  );
}

// Wait for useHotkeyStatus's listener to attach. Probe by emitting an ok=true
// status and looking for delivered > 0; harmless because that's the default.
export async function waitForHotkeyStatusListener(page: Page) {
  await page.waitForFunction(
    () => window.__owEmit("hotkey_status", { ok: true, error: "" }) > 0,
    { timeout: 3000 },
  );
}

// Wait for usePermissionsStatus's listener to attach. Probe by emitting an
// authorized status — same harmless-default trick as the hotkey probe.
export async function waitForPermissionsStatusListener(page: Page) {
  await page.waitForFunction(
    () =>
      window.__owEmit("permissions_status", {
        mic_ok: true,
        mic_state: "authorized",
        error: "",
      }) > 0,
    { timeout: 3000 },
  );
}

// Wait for the dictation hook's listener to have been registered before the
// first tick lands. Otherwise the emit returns 0 and React state stays at INITIAL.
export async function waitForTickListener(page: Page) {
  await page.waitForFunction(
    () => {
      const ok = window.__owEmit("dictation_tick", {
        phase: 0,
        status: "idle",
        status_message: "",
        transcript: "",
        confidence: 0,
        sample_count: 0,
        elapsed_ms: 0,
        error_message: "",
        can_toggle: true,
        is_recording: false,
        level: 0,
        download_bytes_done: 0,
        download_bytes_total: 0,
      });
      return ok > 0;
    },
    { timeout: 3000 },
  );
}

// Push a fresh `audio_device_state` snapshot at the AudioPane subscriber.
// Mirrors `AudioDeviceState` in apps/tauri/src-tauri/src/lib.rs. Defaults
// follow the shim's seeded device fixture so the helper composes cleanly
// with `__owAudioDevice` overrides set earlier in a test.
export interface MockAudioDevice {
  id: string;
  label: string;
  is_default: boolean;
}

export interface MockAudioDeviceState {
  devices?: MockAudioDevice[];
  selected_id?: string | null;
  selected_present?: boolean;
  default_label?: string | null;
}

const DEFAULT_DEVICE_FIXTURE: MockAudioDevice[] = [
  { id: "default-mic", label: "MacBook Pro Microphone", is_default: true },
  { id: "airpods-pro", label: "AirPods Pro", is_default: false },
];

export async function emitDeviceState(
  page: Page,
  state: MockAudioDeviceState = {},
): Promise<number> {
  return page.evaluate(
    ({ partial, fallback }) => {
      const w = window as unknown as {
        __owAudioDevices?: Array<{
          id: string;
          label: string;
          is_default: boolean;
        }>;
        __owAudioDevice?: string | null;
      };
      const devices = partial.devices ?? w.__owAudioDevices ?? fallback;
      const selectedId = partial.selected_id ?? w.__owAudioDevice ?? null;
      const defaultLabel =
        partial.default_label ??
        devices.find((d) => d.is_default)?.label ??
        null;
      const selectedPresent =
        partial.selected_present ??
        (selectedId === null || devices.some((d) => d.id === selectedId));
      return window.__owEmit("audio_device_state", {
        devices,
        selected_id: selectedId,
        selected_present: selectedPresent,
        default_label: defaultLabel,
      });
    },
    { partial: state, fallback: DEFAULT_DEVICE_FIXTURE },
  );
}

// Wait for AudioPane's `audio_device_state` listener to attach. Probe by
// emitting a benign snapshot and asserting at least one delivery — same
// trick as `waitForTickListener`.
export async function waitForDeviceStateListener(page: Page) {
  await page.waitForFunction(
    (fallback) => {
      const w = window as unknown as {
        __owAudioDevices?: Array<{
          id: string;
          label: string;
          is_default: boolean;
        }>;
        __owAudioDevice?: string | null;
      };
      const devices = w.__owAudioDevices ?? fallback;
      const selectedId = w.__owAudioDevice ?? null;
      const defaultLabel = devices.find((d) => d.is_default)?.label ?? null;
      const selectedPresent =
        selectedId === null || devices.some((d) => d.id === selectedId);
      return (
        window.__owEmit("audio_device_state", {
          devices,
          selected_id: selectedId,
          selected_present: selectedPresent,
          default_label: defaultLabel,
        }) > 0
      );
    },
    DEFAULT_DEVICE_FIXTURE,
    { timeout: 3000 },
  );
}

// ---------------------------------------------------------------
// Diagnostics — memory telemetry shim helpers (TASK-62.10).
// ---------------------------------------------------------------

export type MockLifecycleState =
  | "Unloaded"
  | "Loading"
  | "Loaded"
  | "Active"
  | "Releasing";

export interface MockProcessMemory {
  rss_bytes: number;
  peak_rss_bytes: number;
  timestamp_unix_ms: number;
}

export interface MockSystemMemory {
  total_bytes: number;
  used_bytes: number;
  available_bytes: number;
  swap_total_bytes: number;
  swap_used_bytes: number;
}

export interface MockModelMemoryRow {
  label: string;
  state: MockLifecycleState;
  estimated_rss_bytes: number;
  /// Static weight footprint reported by the loader. Defaults to 0
  /// in fixtures that don't care; tests for ANE-resident memory
  /// must set both `claimed_bytes` and `in_process: false`.
  claimed_bytes?: number;
  /// `true` (default) when the claim is already inside RSS; `false`
  /// for Mac ANE-resident weights.
  in_process?: boolean;
}

export interface MockMemoryStats {
  system: MockSystemMemory;
  process: MockProcessMemory;
  models: MockModelMemoryRow[];
}

/// Stash a MemoryStats fixture on the window so the next
/// `telemetry_get_memory` invoke returns this snapshot. Called both
/// before mount (initial paint) and between polls (1 Hz refresh
/// observation).
export async function setMemoryStats(
  page: Page,
  stats: MockMemoryStats,
): Promise<void> {
  await page.evaluate((s) => {
    (window as unknown as { __owMemoryStats?: MockMemoryStats }).__owMemoryStats = s;
  }, stats);
}

/// Fire `model-state-changed` so the diagnostics pane refetches without
/// waiting for the 1 Hz poll. Mirrors the Rust setup in
/// `apps/tauri/src-tauri/src/lib.rs` registering an `on_state_change`
/// callback that emits `{ label, state }`.
export async function emitModelStateChanged(
  page: Page,
  payload: { label: string; state: MockLifecycleState },
): Promise<number> {
  return page.evaluate(
    (value) => window.__owEmit("model-state-changed", value),
    payload,
  );
}

// ---------------------------------------------------------------
// Crash inspector — TASK-78.3 / 78.7 shim helpers.
// ---------------------------------------------------------------

export interface MockCrashSummary {
  id: string;
  ts_unix_ms: number;
  app_version: string;
  os: string;
  message_truncated: string;
  unread: boolean;
  uploaded_at: number | null;
}

export interface MockCrashFile {
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
  recording_state:
    | {
        status_message_at_crash: string;
        duration_ms: number;
        samples_captured: number;
        model_kind: string | null;
        device_id_hash: string | null;
      }
    | null;
  events: Array<{ ts_unix_ms: number; kind: string; data: unknown }>;
}

/// Stash a list of crash summaries. Subsequent `crashes_list` /
/// `crashes_unread_count` invokes mirror this fixture; mark-read /
/// delete mutate it in place so a Playwright spec can observe the
/// state transitions through the same shim.
export async function setCrashes(
  page: Page,
  crashes: MockCrashSummary[],
): Promise<void> {
  await page.evaluate((rows) => {
    (window as unknown as { __owCrashes?: MockCrashSummary[] }).__owCrashes =
      rows;
  }, crashes);
}

/// Optionally seed full crash files by id, so `crashes_read` returns a
/// hand-picked fixture (otherwise the shim synthesises a minimal one
/// from the matching summary).
export async function setCrashFiles(
  page: Page,
  files: Record<string, MockCrashFile>,
): Promise<void> {
  await page.evaluate((map) => {
    (window as unknown as { __owCrashFiles?: Record<string, MockCrashFile> })
      .__owCrashFiles = map;
  }, files);
}

/// Override what `models_storage_path` returns. Otherwise the shim
/// returns a Mac-shaped placeholder string; tests that need to assert
/// on a specific path or platform-shaped value can pin it explicitly.
export async function setModelsStoragePath(
  page: Page,
  path: string,
): Promise<void> {
  await page.evaluate((p) => {
    (window as unknown as { __owModelsStoragePath?: string })
      .__owModelsStoragePath = p;
  }, path);
}

/// Wait for the diagnostics pane to attach its `model-state-changed`
/// listener (via `useMemoryStats`). Probes by emitting an idempotent
/// payload — the listener triggers a refetch but doesn't change state.
export async function waitForModelStateChangedListener(page: Page) {
  await page.waitForFunction(
    () =>
      window.__owEmit("model-state-changed", {
        label: "__probe",
        state: "Unloaded",
      }) > 0,
    { timeout: 3000 },
  );
}

export const test = base.extend({
  page: async ({ page }, use) => {
    await installTauriShim(page);
    await use(page);
  },
});

// Same shim, but boots the SPA as the "pill" window — main.tsx's
// React.lazy switch then loads PillOverlay instead of App.
export const pillTest = base.extend({
  page: async ({ page }, use) => {
    await installTauriShim(page, "pill");
    await use(page);
  },
});

export { expect };
